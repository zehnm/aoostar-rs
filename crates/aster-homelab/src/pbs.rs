// SPDX-License-Identifier: MIT OR Apache-2.0

//! Proxmox Backup Server data source.
//!
//! Datastore usage and availability (`/status/datastore-usage`) and the last run of verify, sync,
//! prune and garbage collection jobs (`/admin/{verify,sync,prune,gc}`). A read-only API token with
//! the `Audit` role is sufficient.
//!
//! The data is refreshed once a minute to keep the PBS API access log small.

use crate::config::PbsConfig;
use crate::sensors::{Level, Problems, Sensors, format_age, format_bytes, percent, truncate};
use crate::tls::{TlsTrust, client_config};
use anyhow::Context;
use log::debug;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::path::Path;
use std::time::{Duration, Instant};

const REFRESH: Duration = Duration::from_secs(60);
const NAME_LEN: usize = 20;

#[derive(Debug, Clone, Deserialize)]
pub struct DatastoreUsage {
    pub store: String,
    pub total: Option<u64>,
    pub used: Option<u64>,
    pub avail: Option<u64>,
    /// Estimated date when the datastore is full, Unix timestamp. Negative or missing if unknown.
    #[serde(rename = "estimated-full-date")]
    pub estimated_full_date: Option<i64>,
    /// Set if the datastore status can't be read, for example a missing disk.
    pub error: Option<String>,
    /// Removable datastores: `mounted` or `notmounted`
    #[serde(rename = "mount-status")]
    pub mount_status: Option<String>,
}

/// Schedule status of a verify, sync, prune or garbage collection job.
#[derive(Debug, Clone, Deserialize)]
pub struct JobStatus {
    pub id: Option<String>,
    pub store: Option<String>,
    pub comment: Option<String>,
    pub schedule: Option<String>,
    /// Task status of the last run: `OK`, `WARNINGS: n` or an error message
    #[serde(rename = "last-run-state")]
    pub last_run_state: Option<String>,
    #[serde(rename = "last-run-endtime")]
    pub last_run_endtime: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub version: Option<String>,
    pub datastores: Vec<DatastoreUsage>,
    /// (kind, job), kind: VERIFY, SYNC, PRUNE or GC
    pub jobs: Vec<(&'static str, JobStatus)>,
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct Version {
    version: String,
    release: Option<String>,
}

pub struct Limits {
    /// Datastore usage warning / critical thresholds in percent
    pub usage: (f64, f64),
    /// A scheduled job without a run for this many days is a warning
    pub job_max_age_days: f64,
    pub now: i64,
}

pub struct Pbs {
    agent: ureq::Agent,
    url: String,
    auth: String,
    usage_limits: (f64, f64),
    job_max_age_days: f64,
    cache: Option<(Instant, Snapshot)>,
}

impl Pbs {
    pub fn new(cfg: &PbsConfig, config_dir: &Path) -> anyhow::Result<Self> {
        let secret = crate::config::secret(
            "token_secret",
            cfg.token_secret.as_deref(),
            cfg.token_secret_file.as_deref(),
            config_dir,
        )?;
        let trust = TlsTrust::new(cfg.tls_fingerprint.as_deref(), cfg.tls_insecure)?;
        let agent = ureq::AgentBuilder::new()
            .tls_config(client_config(&trust)?)
            .timeout(Duration::from_secs(10))
            .build();
        Ok(Self {
            agent,
            url: cfg.url.trim_end_matches('/').to_string(),
            // PBS uses `:` between token id and secret, PVE uses `=`
            auth: format!("PBSAPIToken={}:{secret}", cfg.token_id),
            usage_limits: cfg.usage_limits.unwrap_or((80.0, 90.0)),
            job_max_age_days: cfg.job_max_age_days.unwrap_or(8.0),
            cache: None,
        })
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        let url = format!("{}/api2/json{path}", self.url);
        let response: ApiResponse<T> = self
            .agent
            .get(&url)
            .set("Authorization", &self.auth)
            .call()
            .with_context(|| format!("GET {url}"))?
            .into_json()
            .with_context(|| format!("Invalid response from {path}"))?;
        Ok(response.data)
    }

    /// Job list of an optional endpoint, for example `/admin/gc` on older releases.
    fn jobs(&self, kind: &'static str, path: &str) -> Vec<(&'static str, JobStatus)> {
        match self.get::<Vec<JobStatus>>(path) {
            Ok(jobs) => jobs.into_iter().map(|j| (kind, j)).collect(),
            Err(e) => {
                debug!("Ignoring PBS {path}: {e:#}");
                Vec::new()
            }
        }
    }

    pub fn fetch(&self) -> anyhow::Result<Snapshot> {
        let version: Version = self.get("/version")?;
        let datastores: Vec<DatastoreUsage> = self.get("/status/datastore-usage")?;
        let mut jobs = self.jobs("VERIFY", "/admin/verify");
        jobs.extend(self.jobs("SYNC", "/admin/sync"));
        jobs.extend(self.jobs("PRUNE", "/admin/prune"));
        jobs.extend(self.jobs("GC", "/admin/gc"));
        Ok(Snapshot {
            version: Some(full_version(&version.version, version.release.as_deref())),
            datastores,
            jobs,
        })
    }

    pub fn update(&mut self, sensors: &mut Sensors, problems: &mut Problems) -> anyhow::Result<()> {
        let due = self
            .cache
            .as_ref()
            .is_none_or(|(checked, _)| checked.elapsed() >= REFRESH);
        if due {
            match self.fetch() {
                Ok(snapshot) => self.cache = Some((Instant::now(), snapshot)),
                Err(e) => {
                    self.cache = None;
                    write_offline(sensors, problems);
                    return Err(e);
                }
            }
        }
        if let Some((_, snapshot)) = &self.cache {
            let limits = Limits {
                usage: self.usage_limits,
                job_max_age_days: self.job_max_age_days,
                now: chrono::Utc::now().timestamp(),
            };
            write_sensors(snapshot, &limits, sensors, problems);
        }
        Ok(())
    }
}

/// Full version: `/version` returns major.minor in `version` and the patch level in `release`.
fn full_version(version: &str, release: Option<&str>) -> String {
    match release {
        Some(release)
            if !release.is_empty()
                && release.chars().all(|c| c.is_ascii_digit())
                && version.split('.').count() < 3 =>
        {
            format!("{version}.{release}")
        }
        _ => version.to_string(),
    }
}

/// Display name of a job: its comment, otherwise the datastore. Job ids are generated and
/// meaningless, for example `v-75e236dd-c487`.
fn job_name(job: &JobStatus) -> &str {
    [&job.comment, &job.store, &job.id]
        .into_iter()
        .flatten()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .unwrap_or("?")
}

/// Sensor values for an unreachable or misconfigured backup server.
pub fn write_offline(sensors: &mut Sensors, problems: &mut Problems) {
    sensors.put("pbs_reachable", 0);
    sensors.put_state("pbs_status", "OFFLINE", Level::Bad);
    problems.add(Level::Bad, "BACKUP SERVER UNREACHABLE");
}

/// Convert a PBS snapshot into sensor values.
pub fn write_sensors(
    snapshot: &Snapshot,
    limits: &Limits,
    sensors: &mut Sensors,
    problems: &mut Problems,
) {
    sensors.put("pbs_reachable", 1);
    sensors.put_state("pbs_status", "ONLINE", Level::Ok);
    if let Some(version) = &snapshot.version {
        sensors.put("pbs_version", version);
    }

    let mut stores: Vec<&DatastoreUsage> = snapshot.datastores.iter().collect();
    stores.sort_by(|a, b| a.store.cmp(&b.store));
    let mut available = 0;
    for (idx, store) in stores.iter().enumerate() {
        let key = format!("datastore[{idx}]");
        let name = truncate(&store.store, NAME_LEN);
        sensors.put(format!("{key}_name"), &name);

        let total = store.total.unwrap_or_default();
        let unavailable = store.error.is_some()
            || store.mount_status.as_deref() == Some("notmounted")
            || total == 0;
        if unavailable {
            sensors.put_state(&format!("{key}_status"), "UNAVAILABLE", Level::Bad);
            if let Some(error) = &store.error {
                sensors.put(format!("{key}_error"), truncate(error, 50));
            }
            problems.add(
                Level::Bad,
                format!("DATASTORE {} UNAVAILABLE", name.to_uppercase()),
            );
            continue;
        }
        available += 1;
        sensors.put_state(&format!("{key}_status"), "ONLINE", Level::Ok);

        let used = store.used.unwrap_or_default();
        let used_percent = percent(used, total);
        sensors.put(format!("{key}_used_percent"), format!("{used_percent:.1}"));
        sensors.put(format!("{key}_used"), format_bytes(used));
        sensors.put(format!("{key}_total"), format_bytes(total));
        sensors.put(
            format!("{key}_usage"),
            format!("{} of {}", format_bytes(used), format_bytes(total)),
        );
        if let Some(avail) = store.avail {
            sensors.put(format!("{key}_avail"), format_bytes(avail));
        }
        let (warn, crit) = limits.usage;
        let level = if used_percent >= crit {
            Level::Bad
        } else if used_percent >= warn {
            Level::Warn
        } else {
            Level::Ok
        };
        sensors.put_state(
            &format!("{key}_used_level"),
            format!("{used_percent:.0}%"),
            level,
        );
        if level != Level::Ok {
            problems.add(
                level,
                format!("DATASTORE {} {used_percent:.0}% FULL", name.to_uppercase()),
            );
        }

        // growth estimate, only if within a year
        if let Some(full) = store.estimated_full_date.filter(|t| *t > limits.now)
            && full - limits.now < 365 * 86_400
        {
            sensors.put(
                format!("{key}_full_in"),
                format!("full in {}", format_age(full - limits.now)),
            );
        }
    }
    sensors.put("datastores_total", stores.len());
    sensors.put("datastores_available", available);
    let level = if available < stores.len() {
        Level::Bad
    } else {
        Level::Ok
    };
    sensors.put_state(
        "datastores_summary",
        format!("{available}/{}", stores.len()),
        level,
    );

    write_jobs(&snapshot.jobs, limits, sensors, problems);
}

fn write_jobs(
    jobs: &[(&'static str, JobStatus)],
    limits: &Limits,
    sensors: &mut Sensors,
    problems: &mut Problems,
) {
    let order = |kind: &str| {
        ["VERIFY", "SYNC", "PRUNE", "GC"]
            .iter()
            .position(|k| *k == kind)
    };
    let mut jobs: Vec<&(&'static str, JobStatus)> = jobs.iter().collect();
    jobs.sort_by_key(|(kind, job)| (order(kind), job_name(job).to_lowercase()));

    let mut ok = 0;
    let mut worst = Level::Ok;
    for (idx, (kind, job)) in jobs.iter().enumerate() {
        let key = format!("pbs_job[{idx}]");
        let name = truncate(job_name(job), NAME_LEN);
        sensors.put(format!("{key}_kind"), kind);
        sensors.put(format!("{key}_name"), &name);
        if let Some(store) = &job.store {
            sensors.put(format!("{key}_store"), store);
        }
        let age = job.last_run_endtime.map(|t| limits.now - t);
        if let Some(age) = age {
            sensors.put(format!("{key}_age"), format_age(age));
        }

        let label = format!("PBS {kind} {}", name.to_uppercase());
        let stale = job.schedule.is_some()
            && age.is_some_and(|a| a as f64 > limits.job_max_age_days * 86_400.0);
        let (text, level) = match job.last_run_state.as_deref() {
            None => ("NEVER RUN", Level::Warn),
            Some("OK") if stale => {
                problems.add(Level::Warn, format!("{label} STALE"));
                ("OK", Level::Warn)
            }
            Some("OK") => ("OK", Level::Ok),
            Some(state) if state.starts_with("WARNINGS") => {
                problems.add(Level::Warn, format!("{label} WARNINGS"));
                ("WARNINGS", Level::Warn)
            }
            Some(state) => {
                sensors.put(format!("{key}_error"), truncate(state, 50));
                problems.add(Level::Bad, format!("{label} FAILED"));
                ("FAILED", Level::Bad)
            }
        };
        if level == Level::Ok {
            ok += 1;
        }
        worst = worst.max(level);
        sensors.put_state(&format!("{key}_state"), text, level);
    }
    sensors.put("pbs_jobs_total", jobs.len());
    sensors.put_state("pbs_jobs_summary", format!("{ok}/{}", jobs.len()), worst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const NOW: i64 = 1_763_200_000;
    const DAY: i64 = 86_400;

    fn snapshot() -> Snapshot {
        let v: Value = serde_json::from_str(include_str!("../tests/pbs_snapshot.json")).unwrap();
        let jobs = |kind: &'static str, key: &str| -> Vec<(&'static str, JobStatus)> {
            serde_json::from_value::<Vec<JobStatus>>(v[key].clone())
                .unwrap()
                .into_iter()
                .map(|j| (kind, j))
                .collect()
        };
        let mut all = jobs("GC", "gc");
        all.extend(jobs("VERIFY", "verify"));
        all.extend(jobs("SYNC", "sync"));
        all.extend(jobs("PRUNE", "prune"));
        Snapshot {
            version: Some("4.2.3".into()),
            datastores: serde_json::from_value(v["datastore-usage"].clone()).unwrap(),
            jobs: all,
        }
    }

    fn run(snapshot: &Snapshot) -> (Sensors, Problems) {
        let limits = Limits {
            usage: (80.0, 90.0),
            job_max_age_days: 8.0,
            now: NOW,
        };
        let mut sensors = Sensors::new();
        let mut problems = Problems::default();
        write_sensors(snapshot, &limits, &mut sensors, &mut problems);
        (sensors, problems)
    }

    #[test]
    fn datastores() {
        let (s, _) = run(&snapshot());
        assert_eq!(s.get("pbs_version"), Some("4.2.3"));
        assert_eq!(s.get("datastore[0]_name"), Some("offsite"));
        assert_eq!(s.get("datastore[0]_status_bad"), Some("UNAVAILABLE"));
        assert_eq!(s.get("datastore[0]_used_percent"), None);
        assert_eq!(s.get("datastore[1]_name"), Some("usb"));
        assert_eq!(s.get("datastore[1]_status_ok"), Some("ONLINE"));
        assert_eq!(s.get("datastore[1]_used_percent"), Some("12.5"));
        assert_eq!(s.get("datastore[1]_used_level_ok"), Some("12%"));
        assert_eq!(s.get("datastore[1]_full_in"), Some("full in 200d"));
        assert_eq!(s.get("datastore[1]_usage"), Some("1000 B of 7.81 KB"));
        assert_eq!(s.get("datastores_available"), Some("1"));
        assert_eq!(s.get("datastores_summary_bad"), Some("1/2"));
    }

    #[test]
    fn jobs_sorted_by_kind() {
        let (s, _) = run(&snapshot());
        assert_eq!(s.get("pbs_job[0]_kind"), Some("VERIFY"));
        // named after the datastore, or the comment if set
        assert_eq!(s.get("pbs_job[0]_name"), Some("usb"));
        assert_eq!(s.get("pbs_job[0]_state_ok"), Some("OK"));
        assert_eq!(s.get("pbs_job[0]_age"), Some("2d"));
        assert_eq!(s.get("pbs_job[1]_kind"), Some("SYNC"));
        assert_eq!(s.get("pbs_job[1]_name"), Some("offsite copy"));
        assert_eq!(s.get("pbs_job[1]_state_bad"), Some("FAILED"));
        assert_eq!(s.get("pbs_job[1]_error"), Some("connection refused"));
        assert_eq!(s.get("pbs_job[2]_kind"), Some("PRUNE"));
        assert_eq!(s.get("pbs_job[2]_state_warn"), Some("WARNINGS"));
        // garbage collection is named after its datastore
        assert_eq!(s.get("pbs_job[3]_kind"), Some("GC"));
        assert_eq!(s.get("pbs_job[3]_name"), Some("offsite"));
        assert_eq!(s.get("pbs_job[3]_state_warn"), Some("NEVER RUN"));
        // last run 10 days ago: stale
        assert_eq!(s.get("pbs_job[4]_name"), Some("usb"));
        assert_eq!(s.get("pbs_job[4]_state_warn"), Some("OK"));
        assert_eq!(s.get("pbs_jobs_summary_bad"), Some("1/5"));
    }

    #[test]
    fn problems() {
        let (_, p) = run(&snapshot());
        // unavailable datastore, failed sync, prune warnings, stale GC
        assert_eq!(p.count(), 4);
    }

    #[test]
    fn full_datastore() {
        let snapshot = Snapshot {
            datastores: vec![DatastoreUsage {
                store: "usb".into(),
                total: Some(1000),
                used: Some(950),
                avail: Some(50),
                estimated_full_date: Some(NOW + 3 * DAY),
                error: None,
                mount_status: None,
            }],
            ..Default::default()
        };
        let (s, p) = run(&snapshot);
        assert_eq!(s.get("datastore[0]_used_level_bad"), Some("95%"));
        assert_eq!(s.get("datastore[0]_full_in"), Some("full in 3d"));
        assert_eq!(p.count(), 1);
    }

    #[test]
    fn version() {
        assert_eq!(full_version("4.2", Some("3")), "4.2.3");
        assert_eq!(full_version("4.2.3", Some("3")), "4.2.3");
        assert_eq!(full_version("4.2", None), "4.2");
        assert_eq!(full_version("4.2", Some("")), "4.2");
    }

    #[test]
    fn offline() {
        let mut s = Sensors::new();
        let mut p = Problems::default();
        write_offline(&mut s, &mut p);
        assert_eq!(s.get("pbs_reachable"), Some("0"));
        assert_eq!(s.get("pbs_status_bad"), Some("OFFLINE"));
        assert_eq!(p.count(), 1);
    }
}
