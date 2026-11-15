// SPDX-License-Identifier: MIT OR Apache-2.0

//! TOML configuration file.

use anyhow::{Context, anyhow};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Output sensor file. Can be overridden with `--out`.
    pub out: Option<PathBuf>,
    /// Refresh interval in seconds. Default: 10
    pub refresh: Option<u64>,
    /// One `[proxmox]` table or multiple `[[proxmox]]` entries for standalone hosts.
    /// A cluster only needs a single entry.
    #[serde(default, deserialize_with = "one_or_many")]
    pub proxmox: Vec<ProxmoxConfig>,
    pub truenas: Option<TrueNasConfig>,
    pub pbs: Option<PbsConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PbsConfig {
    /// API base URL, for example `https://10.0.0.5:8007`
    pub url: String,
    /// API token id: `user@realm!tokenname`
    pub token_id: String,
    /// API token secret. Prefer `token_secret_file`!
    pub token_secret: Option<String>,
    /// File containing the API token secret.
    pub token_secret_file: Option<PathBuf>,
    /// SHA-256 fingerprint of the server certificate.
    pub tls_fingerprint: Option<String>,
    /// Accept any server certificate. Only for testing!
    #[serde(default)]
    pub tls_insecure: bool,
    /// Datastore usage warning / critical thresholds in percent. Default: 80 / 90
    pub usage_limits: Option<(f64, f64)>,
    /// A scheduled verify, sync, prune or garbage collection job without a run for this many days
    /// is a warning. Default: 8
    pub job_max_age_days: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxmoxConfig {
    /// Host name shown if the host is unreachable. Default: host part of `url`
    pub name: Option<String>,
    /// API base URL. Default: `https://127.0.0.1:8006`
    #[serde(default = "default_pve_url")]
    pub url: String,
    /// API token id: `user@realm!tokenname`
    pub token_id: String,
    /// API token secret. Prefer `token_secret_file`!
    pub token_secret: Option<String>,
    /// File containing the API token secret.
    pub token_secret_file: Option<PathBuf>,
    /// SHA-256 fingerprint of the server certificate: `pvenode cert info`
    pub tls_fingerprint: Option<String>,
    /// Accept any server certificate. Only for testing!
    #[serde(default)]
    pub tls_insecure: bool,
    /// Guest ids to hide. Templates are always hidden.
    #[serde(default)]
    pub exclude: Vec<u32>,
    /// Guest ids that should always be running. A stopped guest is counted as a problem.
    #[serde(default)]
    pub expect_running: Vec<u32>,
    /// A guest whose newest backup is older than this is a warning. Default: 26 hours
    pub backup_max_age_hours: Option<f64>,
    /// Guest ids without backup monitoring.
    #[serde(default)]
    pub backup_ignore: Vec<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrueNasConfig {
    /// Host name or IP address, optionally with `:port`. Default port: 443
    pub host: String,
    /// User name of the API key. If set, `auth.login_ex` is used, otherwise `auth.login_with_api_key`.
    pub username: Option<String>,
    /// API key. Prefer `api_key_file`!
    pub api_key: Option<String>,
    /// File containing the API key.
    pub api_key_file: Option<PathBuf>,
    /// SHA-256 fingerprint of the server certificate.
    pub tls_fingerprint: Option<String>,
    /// Accept any server certificate. Only for testing!
    #[serde(default)]
    pub tls_insecure: bool,
    /// Disk names to hide, for example a virtual boot disk.
    #[serde(default)]
    pub exclude_disks: Vec<String>,
    /// HDD temperature warning / critical thresholds in °C. Default: 45 / 55
    pub hdd_temp_limits: Option<(f64, f64)>,
    /// SSD and NVMe temperature warning / critical thresholds in °C. Default: 60 / 70
    pub ssd_temp_limits: Option<(f64, f64)>,
    /// Pool usage warning / critical thresholds in percent. Default: 80 / 90
    pub pool_usage_limits: Option<(f64, f64)>,
    /// A pool not scrubbed for this many days is a warning. Default: 40
    pub scrub_max_age_days: Option<f64>,
    /// A replication, cloud sync, rsync or snapshot task that hasn't run successfully for this
    /// many days is a warning. Default: 8
    pub task_max_age_days: Option<f64>,
    /// Names of data protection tasks to hide.
    #[serde(default)]
    pub exclude_tasks: Vec<String>,
}

fn one_or_many<'de, D>(deserializer: D) -> Result<Vec<ProxmoxConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(Box<ProxmoxConfig>),
        Many(Vec<ProxmoxConfig>),
    }
    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(one) => vec![*one],
        OneOrMany::Many(many) => many,
    })
}

fn default_pve_url() -> String {
    "https://127.0.0.1:8006".into()
}

pub fn load(path: &Path) -> anyhow::Result<Config> {
    let data = fs::read_to_string(path).with_context(|| format!("Failed to read {path:?}"))?;
    toml::from_str(&data).with_context(|| format!("Invalid configuration file {path:?}"))
}

/// Resolve a secret from an inline value or a file.
///
/// Relative file paths are resolved against the systemd credentials directory
/// (`$CREDENTIALS_DIRECTORY`, see `LoadCredential=`) if set, otherwise against the configuration
/// file directory.
pub fn secret(
    name: &str,
    inline: Option<&str>,
    file: Option<&Path>,
    config_dir: &Path,
) -> anyhow::Result<String> {
    if let Some(value) = inline {
        return Ok(value.trim().to_string());
    }
    let file = file.ok_or_else(|| anyhow!("Missing {name} or {name}_file"))?;
    let path = if file.is_absolute() {
        file.to_path_buf()
    } else if let Some(dir) = std::env::var_os("CREDENTIALS_DIRECTORY") {
        PathBuf::from(dir).join(file)
    } else {
        config_dir.join(file)
    };
    let value = fs::read_to_string(&path).with_context(|| format!("Failed to read {path:?}"))?;
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(anyhow!("{path:?} is empty"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_example_config() {
        let cfg: Config = toml::from_str(include_str!("../homelab.example.toml")).unwrap();
        assert_eq!(cfg.proxmox.len(), 3);
        let pve = &cfg.proxmox[0];
        assert_eq!(pve.url, "https://127.0.0.1:8006");
        assert!(pve.token_secret_file.is_some());
        assert_eq!(cfg.proxmox[1].name.as_deref(), Some("pve-b"));
        let pbs = cfg.pbs.unwrap();
        assert_eq!(pbs.token_id, "aster@pbs!lcd");
        assert_eq!(pbs.job_max_age_days, Some(8.0));
        let nas = cfg.truenas.unwrap();
        assert_eq!(nas.hdd_temp_limits, Some((50.0, 55.0)));
        assert_eq!(nas.pool_usage_limits, Some((80.0, 90.0)));
    }

    #[test]
    fn single_proxmox_table() {
        let cfg: Config =
            toml::from_str("[proxmox]\ntoken_id = \"a@pve!b\"\ntoken_secret = \"x\"").unwrap();
        assert_eq!(cfg.proxmox.len(), 1);
        let cfg: Config = toml::from_str("refresh = 5").unwrap();
        assert!(cfg.proxmox.is_empty());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(toml::from_str::<Config>("refesh = 5").is_err());
    }
}
