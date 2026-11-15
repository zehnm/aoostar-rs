// SPDX-License-Identifier: MIT OR Apache-2.0

//! TLS configuration for self-signed homelab certificates.
//!
//! Proxmox VE and TrueNAS use self-signed certificates by default. Instead of disabling certificate
//! verification, the SHA-256 fingerprint of the server certificate can be pinned.

use anyhow::{Context, anyhow, bail};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How to trust the server certificate.
#[derive(Debug, Clone)]
pub enum TlsTrust {
    /// Regular certificate validation with the Mozilla root certificates.
    System,
    /// Only accept the server certificate with this SHA-256 fingerprint.
    Fingerprint(Vec<u8>),
    /// Accept any certificate. Only for testing!
    Insecure,
}

impl TlsTrust {
    pub fn new(fingerprint: Option<&str>, insecure: bool) -> anyhow::Result<Self> {
        match (fingerprint, insecure) {
            (Some(fp), _) => Ok(TlsTrust::Fingerprint(parse_fingerprint(fp)?)),
            (None, true) => Ok(TlsTrust::Insecure),
            (None, false) => Ok(TlsTrust::System),
        }
    }
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

pub fn client_config(trust: &TlsTrust) -> anyhow::Result<Arc<ClientConfig>> {
    let provider = provider();
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?;

    let config = match trust {
        TlsTrust::System => {
            let mut roots = RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            builder.with_root_certificates(roots).with_no_client_auth()
        }
        TlsTrust::Fingerprint(fp) => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedVerifier::new(
                Some(fp.clone()),
                provider,
            )))
            .with_no_client_auth(),
        TlsTrust::Insecure => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedVerifier::new(None, provider)))
            .with_no_client_auth(),
    };

    Ok(Arc::new(config))
}

/// Connect to a TLS server and return the SHA-256 fingerprint of its certificate.
///
/// The certificate is not verified! Compare the fingerprint with the one shown in the server's
/// web interface or on its console before pinning it.
pub fn fetch_fingerprint(host: &str, port: u16) -> anyhow::Result<String> {
    let seen = Arc::new(Mutex::new(None));
    let mut verifier = PinnedVerifier::new(None, provider());
    verifier.seen = Some(seen.clone());

    let config = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();

    let server_name = ServerName::try_from(host.to_string())
        .with_context(|| format!("Invalid host name: {host}"))?;
    let mut conn = ClientConnection::new(Arc::new(config), server_name)?;
    let mut tcp = TcpStream::connect((host, port))
        .with_context(|| format!("Failed to connect to {host}:{port}"))?;
    tcp.set_read_timeout(Some(Duration::from_secs(10)))?;
    while conn.is_handshaking() {
        conn.complete_io(&mut tcp)?;
    }

    let fp = seen
        .lock()
        .expect("poisoned")
        .take()
        .ok_or_else(|| anyhow!("Server didn't present a certificate"))?;
    Ok(format_fingerprint(&fp))
}

pub fn format_fingerprint(fp: &[u8]) -> String {
    fp.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Parse a SHA-256 fingerprint in hex notation, with or without `:` separators.
pub fn parse_fingerprint(fp: &str) -> anyhow::Result<Vec<u8>> {
    let hex: String = fp
        .chars()
        .filter(|c| !matches!(c, ':' | ' ' | '-'))
        .collect();
    if hex.len() != 64 {
        bail!("Invalid SHA-256 fingerprint, expected 32 hex bytes: {fp}");
    }
    (0..64)
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .with_context(|| format!("Invalid SHA-256 fingerprint: {fp}"))
        })
        .collect()
}

/// Certificate verifier comparing the leaf certificate fingerprint.
///
/// Handshake signatures are still verified, so the server must own the pinned certificate's key.
#[derive(Debug)]
struct PinnedVerifier {
    /// Expected SHA-256 fingerprint, `None` accepts any certificate.
    expected: Option<Vec<u8>>,
    provider: Arc<CryptoProvider>,
    /// Optional storage for the seen fingerprint.
    seen: Option<Arc<Mutex<Option<Vec<u8>>>>>,
}

impl PinnedVerifier {
    fn new(expected: Option<Vec<u8>>, provider: Arc<CryptoProvider>) -> Self {
        Self {
            expected,
            provider,
            seen: None,
        }
    }
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = ring::digest::digest(&ring::digest::SHA256, end_entity.as_ref());
        if let Some(seen) = &self.seen {
            *seen.lock().expect("poisoned") = Some(fp.as_ref().to_vec());
        }
        match &self.expected {
            None => Ok(ServerCertVerified::assertion()),
            Some(expected) if expected.as_slice() == fp.as_ref() => {
                Ok(ServerCertVerified::assertion())
            }
            Some(_) => Err(rustls::Error::General(format!(
                "Server certificate fingerprint mismatch: {}",
                format_fingerprint(fp.as_ref())
            ))),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fingerprint_with_and_without_separators() {
        let colon = "AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89";
        let plain = colon.replace(':', "").to_lowercase();
        let a = parse_fingerprint(colon).unwrap();
        assert_eq!(a.len(), 32);
        assert_eq!(a, parse_fingerprint(&plain).unwrap());
        assert_eq!(format_fingerprint(&a), colon);
    }

    #[test]
    fn parse_fingerprint_rejects_invalid_input() {
        assert!(parse_fingerprint("AB:CD").is_err());
        assert!(parse_fingerprint(&"zz".repeat(32)).is_err());
    }
}
