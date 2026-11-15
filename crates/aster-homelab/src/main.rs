// SPDX-License-Identifier: MIT OR Apache-2.0

//! Proxmox VE, Proxmox Backup Server and TrueNAS sensor value provider for `asterctl`.

#![forbid(non_ascii_idents)]
#![deny(unsafe_code)]

mod config;
mod pbs;
mod proxmox;
mod sensors;
mod tls;
mod truenas;

use crate::sensors::{Problems, Sensors};
use anyhow::{Context, anyhow};
use clap::Parser;
use env_logger::Env;
use log::{info, warn};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Proxmox VE and TrueNAS sensor value provider for asterctl.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Configuration file.
    #[arg(short, long, default_value = "homelab.toml")]
    config: PathBuf,

    /// Output sensor file, overrides the `out` setting in the configuration file.
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Print values in console. The `out` file of the configuration is not written, unless `--out`
    /// is specified as well.
    #[arg(long)]
    console: bool,

    /// Run once and exit.
    #[arg(long)]
    once: bool,

    /// Print the SHA-256 certificate fingerprint of a TLS server ("host" or "host:port") and exit.
    ///
    /// The certificate is NOT verified. Compare it with the fingerprint shown on the server
    /// before pinning it with `tls_fingerprint`.
    #[arg(long, value_name = "HOST")]
    print_fingerprint: Option<String>,
}

/// Logs source errors only when they change, to avoid flooding the journal.
#[derive(Default)]
struct ErrorState(Option<String>);

impl ErrorState {
    fn report(&mut self, source: &str, result: anyhow::Result<()>) {
        match result {
            Ok(_) => {
                if self.0.take().is_some() {
                    info!("{source} is reachable again");
                }
            }
            Err(e) => {
                let msg = format!("{e:#}");
                if self.0.as_ref() != Some(&msg) {
                    warn!("{source} update failed: {msg}");
                    self.0 = Some(msg);
                }
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    if let Some(host) = args.print_fingerprint {
        let (host, port) = match host.rsplit_once(':') {
            Some((h, p)) if !h.contains(':') => (h.to_string(), p.parse()?),
            _ => (host, 443),
        };
        println!("{}", tls::fetch_fingerprint(&host, port)?);
        return Ok(());
    }

    let cfg = config::load(&args.config)?;
    let config_dir = args
        .config
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    // console mode is for testing: don't write the service's sensor file
    let out = if args.console {
        args.out
    } else {
        args.out.or(cfg.out.clone())
    };
    if out.is_none() && !args.console {
        return Err(anyhow!("No output file configured. Use --out or --console"));
    }
    if let Some(parent) = out.as_deref().and_then(Path::parent)
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create output directory {parent:?}"))?;
    }

    // A misconfigured source, for example a missing secret file, is shown as unreachable.
    // It must not prevent the other sources from working.
    let mut pves: Vec<(String, Result<proxmox::Proxmox, String>)> = cfg
        .proxmox
        .iter()
        .map(|c| {
            let name = proxmox::display_name(c);
            let pve = proxmox::Proxmox::new(c, &config_dir)
                .map_err(|e| format!("Invalid [[proxmox]] configuration: {e:#}"));
            (name, pve)
        })
        .collect();
    let mut nas: Option<Result<truenas::TrueNas, String>> = cfg.truenas.as_ref().map(|c| {
        truenas::TrueNas::new(c, &config_dir)
            .map_err(|e| format!("Invalid [truenas] configuration: {e:#}"))
    });
    let mut pbs: Option<Result<pbs::Pbs, String>> = cfg.pbs.as_ref().map(|c| {
        pbs::Pbs::new(c, &config_dir).map_err(|e| format!("Invalid [pbs] configuration: {e:#}"))
    });
    if pves.is_empty() && nas.is_none() && pbs.is_none() {
        return Err(anyhow!(
            "None of [proxmox], [truenas] or [pbs] is configured"
        ));
    }

    let refresh = Duration::from_secs(cfg.refresh.unwrap_or(10).max(2));
    info!(
        "Starting aster-homelab: proxmox={:?}, truenas={}, pbs={}, refresh={}s",
        pves.iter().map(|(name, _)| name).collect::<Vec<_>>(),
        nas.is_some(),
        pbs.is_some(),
        refresh.as_secs()
    );

    let mut pve_errors: Vec<ErrorState> = pves.iter().map(|_| ErrorState::default()).collect();
    let mut nas_errors = ErrorState::default();
    let mut pbs_errors = ErrorState::default();
    loop {
        let start = Instant::now();
        let mut values = Sensors::new();
        let mut problems = Problems::default();

        if !pves.is_empty() {
            let mut resources = Vec::new();
            let mut jobs = Vec::new();
            let mut offline = Vec::new();
            for ((name, pve), errors) in pves.iter_mut().zip(pve_errors.iter_mut()) {
                let result = match pve {
                    Ok(pve) => pve.fetch(),
                    Err(e) => Err(anyhow!("{e}")),
                };
                match result {
                    Ok(host) => {
                        errors.report(name, Ok(()));
                        resources.extend(host.resources);
                        jobs.extend(host.jobs);
                    }
                    Err(e) => {
                        errors.report(name, Err(e));
                        offline.push(name.clone());
                    }
                }
            }
            proxmox::write_sensors(
                &resources,
                &jobs,
                pves.len(),
                &offline,
                chrono::Utc::now().timestamp(),
                &mut values,
                &mut problems,
            );
        }
        match &mut nas {
            Some(Ok(nas)) => nas_errors.report("TrueNAS", nas.update(&mut values, &mut problems)),
            Some(Err(e)) => {
                truenas::write_offline(&mut values, &mut problems);
                nas_errors.report("TrueNAS", Err(anyhow!("{e}")));
            }
            None => {}
        }
        match &mut pbs {
            Some(Ok(pbs)) => {
                pbs_errors.report("Backup server", pbs.update(&mut values, &mut problems))
            }
            Some(Err(e)) => {
                pbs::write_offline(&mut values, &mut problems);
                pbs_errors.report("Backup server", Err(anyhow!("{e}")));
            }
            None => {}
        }
        problems.write(&mut values, 3);
        values.put("homelab_updated", chrono::Local::now().format("%H:%M:%S"));

        if let Some(out) = &out {
            write_sensor_file(out, &values)?;
        }
        if args.console {
            for (key, value) in values.iter() {
                println!("{key}: {value}");
            }
            println!();
        }

        if args.once {
            break;
        }
        if let Some(remaining) = refresh.checked_sub(start.elapsed()) {
            sleep(remaining);
        }
    }

    Ok(())
}

/// Atomically replace the sensor file: write a temporary file in the same directory and rename.
fn write_sensor_file(out: &Path, values: &Sensors) -> anyhow::Result<()> {
    let file_name = out
        .file_name()
        .ok_or_else(|| anyhow!("Invalid output file {out:?}"))?
        .to_string_lossy();
    let tmp = out.with_file_name(format!(".{file_name}.tmp"));
    {
        let mut file =
            fs::File::create(&tmp).with_context(|| format!("Failed to create {tmp:?}"))?;
        file.set_permissions(fs::Permissions::from_mode(0o644))?;
        let mut writer = std::io::BufWriter::new(&mut file);
        writeln!(writer, "# aster-homelab sensor values")?;
        for (key, value) in values.iter() {
            writeln!(writer, "{key}: {value}")?;
        }
        writer.flush()?;
    }
    fs::rename(&tmp, out).with_context(|| format!("Failed to write {out:?}"))?;
    Ok(())
}
