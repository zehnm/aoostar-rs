// SPDX-License-Identifier: MIT OR Apache-2.0

//! Proxmox VE API data source.
//!
//! Uses a `GET /api2/json/cluster/resources` request, which returns all nodes, guests and
//! storages of a standalone node or of a whole cluster. A read-only API token with the
//! `PVEAuditor` role is sufficient.
//!
//! Backup status is refreshed every few minutes from the `vzdump` task history of each node: the
//! result of the last backup job, and the last successful and the last attempted backup of each
//! guest from the task logs. Listing backup files on storages is not possible with read-only
//! permissions: Proxmox requires `VM.Backup` and `Datastore.AllocateSpace` for that.

use crate::config::ProxmoxConfig;
use crate::sensors::{
    Level, Problems, Sensors, format_age, format_bytes, format_uptime, percent, truncate,
};
use crate::tls::{TlsTrust, client_config};
use anyhow::Context;
use log::{debug, warn};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};

/// Backup status refresh interval.
const BACKUP_REFRESH: Duration = Duration::from_secs(300);

/// Maximum guest name length on the display
const NAME_LEN: usize = 14;

#[derive(Debug, Deserialize)]
pub struct Resource {
    #[serde(rename = "type")]
    pub kind: String,
    pub vmid: Option<u32>,
    pub name: Option<String>,
    pub node: Option<String>,
    pub storage: Option<String>,
    pub status: Option<String>,
    pub template: Option<u8>,
    pub shared: Option<u8>,
    pub cpu: Option<f64>,
    pub maxcpu: Option<f64>,
    pub mem: Option<u64>,
    pub maxmem: Option<u64>,
    pub disk: Option<u64>,
    pub maxdisk: Option<u64>,
    pub uptime: Option<u64>,
    /// Guest must be running, set from the host configuration.
    #[serde(skip)]
    pub expect_running: bool,
    /// Newest backup of a guest, set from the backup status.
    #[serde(skip)]
    pub backup: Backup,
    /// Maximum backup age in seconds, set from the host configuration.
    #[serde(skip)]
    pub backup_max_age: i64,
}

/// Backup status of a guest.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Backup {
    /// Backup status not available (yet)
    #[default]
    Unknown,
    /// Backup monitoring disabled for this guest
    Ignored,
    /// No backup in the task history
    None,
    /// Backups in the task history
    Known(GuestBackup),
}

/// Backup history of a guest.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GuestBackup {
    /// End of the backup job with the last successful backup, as Unix timestamp
    pub last_ok: Option<i64>,
    /// The most recent backup attempt failed
    pub last_failed: bool,
}

/// Last `vzdump` task of a node.
#[derive(Debug, Clone)]
pub struct BackupJob {
    pub node: String,
    /// Task status: `OK`, `WARNINGS: n` or an error message. `None` while running.
    pub status: Option<String>,
    pub start: i64,
    pub end: Option<i64>,
    /// Number of guests whose backup failed in this job
    pub failed_guests: usize,
}

/// Resources and backup jobs of one host or cluster.
pub struct HostData {
    pub resources: Vec<Resource>,
    pub jobs: Vec<BackupJob>,
}

#[derive(Debug, Deserialize)]
struct Task {
    upid: String,
    starttime: i64,
    endtime: Option<i64>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LogLine {
    t: String,
}

/// Number of `vzdump` tasks per node to evaluate, covering a few weeks of daily or weekly jobs.
const TASK_HISTORY: usize = 50;

#[derive(Default)]
struct BackupCache {
    checked: Option<Instant>,
    /// Backup history per guest id, `None` until the first successful query
    guests: Option<HashMap<u32, GuestBackup>>,
    jobs: Vec<BackupJob>,
    /// Parsed logs of finished tasks by UPID: (guest id, backup successful)
    logs: HashMap<String, Vec<(u32, bool)>>,
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    data: T,
}

pub struct Proxmox {
    agent: ureq::Agent,
    url: String,
    auth: String,
    exclude: HashSet<u32>,
    expect_running: HashSet<u32>,
    backup_max_age: i64,
    backup_ignore: HashSet<u32>,
    backups: BackupCache,
}

impl Proxmox {
    pub fn new(cfg: &ProxmoxConfig, config_dir: &Path) -> anyhow::Result<Self> {
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
        let url = cfg.url.trim_end_matches('/').to_string();
        Ok(Self {
            agent,
            url,
            auth: format!("PVEAPIToken={}={secret}", cfg.token_id),
            exclude: cfg.exclude.iter().copied().collect(),
            expect_running: cfg.expect_running.iter().copied().collect(),
            backup_max_age: (cfg.backup_max_age_hours.unwrap_or(26.0) * 3600.0) as i64,
            backup_ignore: cfg.backup_ignore.iter().copied().collect(),
            backups: BackupCache::default(),
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

    /// Get all resources of this host (or cluster) without excluded guests, and the backup status.
    pub fn fetch(&mut self) -> anyhow::Result<HostData> {
        let resources: Vec<Resource> = self.get("/cluster/resources")?;

        let nodes: Vec<String> = resources
            .iter()
            .filter(|r| r.kind == "node" && r.status.as_deref() == Some("online"))
            .filter_map(|r| r.node.clone())
            .collect();
        self.refresh_backups(&nodes);

        let resources = resources
            .into_iter()
            .filter(|r| !r.vmid.is_some_and(|id| self.exclude.contains(&id)))
            .map(|mut r| {
                if let Some(vmid) = r.vmid {
                    r.expect_running = self.expect_running.contains(&vmid);
                    r.backup_max_age = self.backup_max_age;
                    r.backup = if self.backup_ignore.contains(&vmid) {
                        Backup::Ignored
                    } else {
                        match &self.backups.guests {
                            None => Backup::Unknown,
                            Some(guests) => guests
                                .get(&vmid)
                                .map_or(Backup::None, |b| Backup::Known(*b)),
                        }
                    };
                }
                r
            })
            .collect();

        Ok(HostData {
            resources,
            jobs: self.backups.jobs.clone(),
        })
    }

    fn refresh_backups(&mut self, nodes: &[String]) {
        if self
            .backups
            .checked
            .is_some_and(|t| t.elapsed() < BACKUP_REFRESH)
        {
            return;
        }
        self.backups.checked = Some(Instant::now());
        match self.query_backups(nodes) {
            Ok((guests, jobs)) => {
                debug!("Backup history found for {} guests", guests.len());
                self.backups.guests = Some(guests);
                self.backups.jobs = jobs;
            }
            Err(e) => warn!("Failed to get backup status from {}: {e:#}", self.url),
        }
    }

    fn query_backups(
        &mut self,
        nodes: &[String],
    ) -> anyhow::Result<(HashMap<u32, GuestBackup>, Vec<BackupJob>)> {
        let mut guests: HashMap<u32, GuestBackup> = HashMap::new();
        // guests whose most recent attempt has been evaluated
        let mut attempted: HashSet<u32> = HashSet::new();
        let mut jobs = Vec::new();
        let mut current_upids = HashSet::new();

        for node in nodes {
            let mut tasks: Vec<Task> = self.get(&format!(
                "/nodes/{node}/tasks?typefilter=vzdump&limit={TASK_HISTORY}"
            ))?;
            tasks.sort_by(|a, b| b.starttime.cmp(&a.starttime)); // newest first

            for (idx, task) in tasks.iter().enumerate() {
                current_upids.insert(task.upid.clone());
                let results = match self.task_results(node, task) {
                    Ok(results) => results,
                    Err(e) => {
                        warn!("Failed to read backup task log {}: {e:#}", task.upid);
                        continue;
                    }
                };
                let finished = task.endtime.unwrap_or(task.starttime);
                for &(vmid, ok) in &results {
                    let guest = guests.entry(vmid).or_default();
                    if attempted.insert(vmid) {
                        guest.last_failed = !ok;
                    }
                    if ok {
                        guest.last_ok = Some(guest.last_ok.map_or(finished, |t| t.max(finished)));
                    }
                }
                if idx == 0 {
                    jobs.push(BackupJob {
                        node: node.clone(),
                        status: task.status.clone(),
                        start: task.starttime,
                        end: task.endtime,
                        failed_guests: results.iter().filter(|(_, ok)| !ok).count(),
                    });
                }
            }
        }

        // forget logs of tasks that dropped out of the history
        self.backups
            .logs
            .retain(|upid, _| current_upids.contains(upid));
        Ok((guests, jobs))
    }

    /// Backup results of a task, from the cache for finished tasks.
    fn task_results(&mut self, node: &str, task: &Task) -> anyhow::Result<Vec<(u32, bool)>> {
        if let Some(results) = self.backups.logs.get(&task.upid) {
            return Ok(results.clone());
        }
        let lines: Vec<LogLine> = self.get(&format!(
            "/nodes/{node}/tasks/{}/log?start=0&limit=100000",
            task.upid
        ))?;
        let results = parse_vzdump_log(lines.iter().map(|l| l.t.as_str()));
        if task.status.is_some() {
            // finished: the log doesn't change anymore
            self.backups.logs.insert(task.upid.clone(), results.clone());
        }
        Ok(results)
    }
}

/// Backup results of a `vzdump` task log: (guest id, successful) per guest.
///
/// Relevant log lines:
/// - `INFO: Finished Backup of VM 100 (00:01:23)`
/// - `ERROR: Backup of VM 101 failed - unable to ...`
pub fn parse_vzdump_log<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<(u32, bool)> {
    let vmid = |rest: &str| {
        rest.split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|id| id.parse::<u32>().ok())
    };
    let mut results = Vec::new();
    for line in lines {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("INFO: Finished Backup of VM ") {
            if let Some(id) = vmid(rest) {
                results.push((id, true));
            }
        } else if let Some(rest) = line.strip_prefix("ERROR: Backup of VM ")
            && rest.contains(" failed")
            && let Some(id) = vmid(rest)
        {
            results.push((id, false));
        }
    }
    results
}

/// Host name used for logging and for unreachable hosts.
pub fn display_name(cfg: &ProxmoxConfig) -> String {
    cfg.name
        .clone()
        .unwrap_or_else(|| host_of(cfg.url.trim_end_matches('/')))
}

fn host_of(url: &str) -> String {
    let host = url.split("://").last().unwrap_or(url);
    let host = host.split('/').next().unwrap_or(host);
    match host.rsplit_once(':') {
        Some((name, port)) if !name.contains(':') && port.parse::<u16>().is_ok() => name.into(),
        _ => host.into(),
    }
}

/// Convert the resources of all reachable hosts into sensor values.
///
/// # Arguments
///
/// * `resources`: combined resources of all reachable hosts
/// * `hosts`: number of configured hosts
/// * `offline_hosts`: names of unreachable hosts
pub fn write_sensors(
    resources: &[Resource],
    jobs: &[BackupJob],
    hosts: usize,
    offline_hosts: &[String],
    now: i64,
    sensors: &mut Sensors,
    problems: &mut Problems,
) {
    let reachable = hosts - offline_hosts.len();
    sensors.put("pve_reachable", u8::from(reachable > 0));
    let level = if offline_hosts.is_empty() {
        Level::Ok
    } else if reachable > 0 {
        Level::Warn
    } else {
        Level::Bad
    };
    let status = match (hosts, level) {
        (1, Level::Ok) => "ONLINE".to_string(),
        (1, _) => "OFFLINE".to_string(),
        _ => format!("{reachable}/{hosts} UP"),
    };
    sensors.put_state("pve_status", status, level);
    for host in offline_hosts {
        problems.add(Level::Bad, format!("{} UNREACHABLE", host.to_uppercase()));
    }

    // guests: VMs and containers, sorted by node and id for stable positions
    let mut guests: Vec<&Resource> = resources
        .iter()
        .filter(|r| matches!(r.kind.as_str(), "qemu" | "lxc"))
        .filter(|r| r.template.unwrap_or_default() == 0 && r.vmid.is_some())
        .collect();
    guests.sort_by(|a, b| (&a.node, a.vmid).cmp(&(&b.node, b.vmid)));

    // nodes, including unreachable standalone hosts
    let mut nodes: Vec<(&str, Option<&Resource>)> = resources
        .iter()
        .filter(|r| r.kind == "node")
        .map(|r| (r.node.as_deref().unwrap_or("?"), Some(r)))
        .collect();
    for host in offline_hosts {
        if !nodes.iter().any(|(name, _)| name == host) {
            nodes.push((host.as_str(), None));
        }
    }
    nodes.sort_by(|a, b| a.0.cmp(b.0));
    let multi_node = nodes.len() > 1;

    let mut online = 0;
    for (idx, (name, node)) in nodes.iter().enumerate() {
        let key = format!("pve_node[{idx}]");
        sensors.put(format!("{key}_name"), name);
        match node {
            Some(node) if node.status.as_deref() == Some("online") => {
                online += 1;
                sensors.put_state(&format!("{key}_status"), "ONLINE", Level::Ok);
                write_usage(sensors, &key, node);
            }
            Some(_) => {
                sensors.put_state(&format!("{key}_status"), "OFFLINE", Level::Bad);
                problems.add(Level::Bad, format!("NODE {} OFFLINE", name.to_uppercase()));
            }
            None => sensors.put_state(&format!("{key}_status"), "UNREACHABLE", Level::Bad),
        }
        if node.is_some() {
            let on_node = guests.iter().filter(|g| g.node.as_deref() == Some(name));
            let total = on_node.clone().count();
            let running = on_node
                .filter(|g| g.status.as_deref() == Some("running"))
                .count();
            sensors.put(format!("{key}_guests"), format!("{running}/{total}"));
        }
    }
    sensors.put("pve_nodes_total", nodes.len());
    sensors.put("pve_nodes_online", online);

    let (mut vms, mut vms_running, mut cts, mut cts_running) = (0, 0, 0, 0);
    let mut backups = BackupSummary::default();
    for (idx, guest) in guests.iter().enumerate() {
        let key = format!("guest[{idx}]");
        let vmid = guest.vmid.unwrap_or_default();
        let is_vm = guest.kind == "qemu";
        let status = guest.status.as_deref().unwrap_or("unknown");
        let running = status == "running";
        let name = guest.name.clone().unwrap_or_else(|| vmid.to_string());

        if is_vm {
            vms += 1;
            vms_running += running as u32;
        } else {
            cts += 1;
            cts_running += running as u32;
        }

        let kind = if is_vm { "VM" } else { "CT" };
        sensors.put(format!("{key}_id"), vmid);
        sensors.put(format!("{key}_name"), truncate(&name, NAME_LEN));
        sensors.put(format!("{key}_type"), kind);
        // short tag for the panel: guest type and the host if there are several
        match (&guest.node, multi_node) {
            (Some(node), true) => sensors.put(format!("{key}_tag"), format!("{kind} {node}")),
            _ => sensors.put(format!("{key}_tag"), kind),
        }
        if let Some(node) = &guest.node {
            sensors.put(format!("{key}_node"), node);
        }

        let level = match status {
            "running" => Level::Ok,
            "stopped" if guest.expect_running => Level::Bad,
            _ => Level::Warn,
        };
        sensors.put_state(&format!("{key}_status"), status.to_uppercase(), level);
        if level == Level::Bad {
            problems.add(
                Level::Bad,
                format!(
                    "{} {}",
                    truncate(&name, NAME_LEN).to_uppercase(),
                    status.to_uppercase()
                ),
            );
        }

        if running {
            write_usage(sensors, &key, guest);
        }
        backups.add(&key, &name, guest, now, sensors, problems);
    }
    backups.write(sensors);
    write_backup_jobs(jobs, now, sensors, problems);
    sensors.put("guests_total", guests.len());
    sensors.put("guests_running", vms_running + cts_running);
    sensors.put(
        "guests_summary",
        format!("{}/{}", vms_running + cts_running, guests.len()),
    );
    sensors.put("vms_total", vms);
    sensors.put("vms_running", vms_running);
    sensors.put("cts_total", cts);
    sensors.put("cts_running", cts_running);

    // storages: shared storages are listed once per node
    let mut seen = HashSet::new();
    let mut storages: Vec<&Resource> = resources
        .iter()
        .filter(|r| r.kind == "storage" && r.status.as_deref() == Some("available"))
        .filter(|r| r.maxdisk.unwrap_or_default() > 0)
        .filter(|r| {
            let name = r.storage.clone().unwrap_or_default();
            if r.shared.unwrap_or_default() == 1 {
                seen.insert(name)
            } else {
                seen.insert(format!("{}/{name}", r.node.as_deref().unwrap_or_default()))
            }
        })
        .collect();
    storages.sort_by(|a, b| (&a.node, &a.storage).cmp(&(&b.node, &b.storage)));
    for (idx, storage) in storages.iter().enumerate() {
        let key = format!("pve_storage[{idx}]");
        let used = storage.disk.unwrap_or_default();
        let total = storage.maxdisk.unwrap_or_default();
        sensors.put(
            format!("{key}_name"),
            storage.storage.as_deref().unwrap_or("?"),
        );
        if let Some(node) = &storage.node {
            sensors.put(format!("{key}_node"), node);
        }
        sensors.put(
            format!("{key}_used_percent"),
            format!("{:.1}", percent(used, total)),
        );
        sensors.put(format!("{key}_used"), format_bytes(used));
        sensors.put(format!("{key}_total"), format_bytes(total));
    }
    sensors.put("pve_storages_total", storages.len());
}

/// Backup freshness of all guests.
#[derive(Default)]
struct BackupSummary {
    /// guests with at least one backup
    with_backup: usize,
    fresh: usize,
    none: usize,
    /// oldest newest-backup: (age, name)
    oldest: Option<(i64, String)>,
}

impl BackupSummary {
    fn add(
        &mut self,
        key: &str,
        name: &str,
        guest: &Resource,
        now: i64,
        sensors: &mut Sensors,
        problems: &mut Problems,
    ) {
        let key = format!("{key}_backup");
        match guest.backup {
            Backup::Known(backup) => {
                self.with_backup += 1;
                let age = backup.last_ok.map(|t| now - t);
                if let Some(age) = age {
                    sensors.put(format!("{key}_age_hours"), age / 3600);
                    if self.oldest.as_ref().is_none_or(|(oldest, _)| age > *oldest) {
                        self.oldest = Some((age, truncate(name, NAME_LEN)));
                    }
                }
                let upper_name = truncate(name, NAME_LEN).to_uppercase();
                if backup.last_failed {
                    sensors.put_state(&key, "FAILED", Level::Bad);
                    problems.add(Level::Bad, format!("{upper_name} BACKUP FAILED"));
                } else if let Some(age) = age
                    && age <= guest.backup_max_age
                {
                    self.fresh += 1;
                    sensors.put_state(&key, format_age(age), Level::Ok);
                } else if let Some(age) = age {
                    sensors.put_state(&key, format_age(age), Level::Warn);
                    problems.add(
                        Level::Warn,
                        format!("{upper_name} BACKUP {} OLD", format_age(age).to_uppercase()),
                    );
                }
            }
            Backup::None => {
                self.none += 1;
                // informational: not every guest needs a backup, see `backup_ignore`
                sensors.put(&key, "NONE");
                sensors.put(format!("{key}_none"), "NONE");
            }
            Backup::Unknown | Backup::Ignored => {}
        }
    }

    fn write(&self, sensors: &mut Sensors) {
        if self.with_backup + self.none == 0 {
            return;
        }
        let level = if self.fresh < self.with_backup {
            Level::Warn
        } else {
            Level::Ok
        };
        sensors.put_state(
            "backups_summary",
            format!("{}/{}", self.fresh, self.with_backup),
            level,
        );
        sensors.put("backups_fresh", self.fresh);
        sensors.put("backups_total", self.with_backup);
        sensors.put("backups_none", self.none);
        if let Some((age, name)) = &self.oldest {
            sensors.put("backups_oldest_name", name);
            sensors.put("backups_oldest_age", format_age(*age));
        }
    }
}

/// Result of the last backup job of each node.
fn write_backup_jobs(jobs: &[BackupJob], now: i64, sensors: &mut Sensors, problems: &mut Problems) {
    let mut jobs: Vec<&BackupJob> = jobs.iter().collect();
    jobs.sort_by(|a, b| a.node.cmp(&b.node));
    for (idx, job) in jobs.iter().enumerate() {
        let key = format!("pve_backup_job[{idx}]");
        let node = job.node.to_uppercase();
        sensors.put(format!("{key}_node"), &job.node);
        sensors.put(
            format!("{key}_age"),
            format_age(now - job.end.unwrap_or(job.start)),
        );
        match job.status.as_deref() {
            None => sensors.put_state(&format!("{key}_state"), "RUNNING", Level::Ok),
            Some("OK") => sensors.put_state(&format!("{key}_state"), "OK", Level::Ok),
            Some(status) if status.starts_with("WARNINGS") => {
                sensors.put_state(&format!("{key}_state"), "WARNINGS", Level::Warn);
                problems.add(Level::Warn, format!("BACKUP JOB {node} WARNINGS"));
            }
            Some(status) => {
                sensors.put_state(&format!("{key}_state"), "FAILED", Level::Bad);
                sensors.put(format!("{key}_error"), truncate(status, 40));
                if job.failed_guests > 0 {
                    sensors.put(format!("{key}_failed_guests"), job.failed_guests);
                } else {
                    // failing guests are reported individually, avoid counting them twice
                    problems.add(Level::Bad, format!("BACKUP JOB {node} FAILED"));
                }
            }
        }
    }
}

fn write_usage(sensors: &mut Sensors, key: &str, r: &Resource) {
    if let Some(cpus) = r.maxcpu {
        sensors.put(format!("{key}_cpus"), cpus);
    }
    if let Some(cpu) = r.cpu {
        sensors.put(format!("{key}_cpu_percent"), format!("{:.1}", cpu * 100.0));
    }
    if let (Some(mem), Some(maxmem)) = (r.mem, r.maxmem) {
        sensors.put(
            format!("{key}_mem_percent"),
            format!("{:.1}", percent(mem, maxmem)),
        );
        sensors.put(format!("{key}_mem"), format_bytes(mem));
        sensors.put(format!("{key}_mem_total"), format_bytes(maxmem));
    }
    if let Some(uptime) = r.uptime {
        sensors.put(format!("{key}_uptime"), format_uptime(uptime));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESOURCES: &str = include_str!("../tests/pve_cluster_resources.json");

    fn resources(node: &str) -> Vec<Resource> {
        let json = RESOURCES.replace("pve-a", node);
        let response: ApiResponse<Vec<Resource>> = serde_json::from_str(&json).unwrap();
        response.data
    }

    fn run(resources: &[Resource], hosts: usize, offline: &[&str]) -> (Sensors, Problems) {
        let offline: Vec<String> = offline.iter().map(|s| s.to_string()).collect();
        let mut sensors = Sensors::new();
        let mut problems = Problems::default();
        write_sensors(
            resources,
            &[],
            hosts,
            &offline,
            NOW,
            &mut sensors,
            &mut problems,
        );
        (sensors, problems)
    }

    const NOW: i64 = 1_763_200_000;
    const HOUR: i64 = 3600;

    fn with_backups(mut resources: Vec<Resource>) -> Vec<Resource> {
        let ok = |hours: i64| {
            Backup::Known(GuestBackup {
                last_ok: Some(NOW - hours * HOUR),
                last_failed: false,
            })
        };
        for r in resources.iter_mut() {
            r.backup_max_age = 26 * HOUR;
            r.backup = match r.vmid {
                Some(100) => ok(5),
                Some(101) => ok(3 * 24),
                Some(102) => Backup::None,
                Some(200) => Backup::Ignored,
                _ => Backup::Unknown,
            };
        }
        resources
    }

    fn job(node: &str, status: Option<&str>, hours_ago: i64) -> BackupJob {
        BackupJob {
            node: node.to_string(),
            status: status.map(str::to_string),
            start: NOW - hours_ago * HOUR - 600,
            end: status.map(|_| NOW - hours_ago * HOUR),
            failed_guests: 0,
        }
    }

    #[test]
    fn guests_sorted_by_id_without_templates() {
        let (s, p) = run(&resources("pve-a"), 1, &[]);
        assert_eq!(s.get("guest[0]_name"), Some("opnsense"));
        assert_eq!(s.get("guest[0]_type"), Some("VM"));
        assert_eq!(s.get("guest[0]_tag"), Some("VM"));
        assert_eq!(s.get("guest[1]_name"), Some("truenas"));
        assert_eq!(s.get("guest[2]_name"), Some("homeassistant"));
        assert_eq!(s.get("guest[3]_name"), Some("pihole"));
        assert_eq!(s.get("guest[3]_type"), Some("CT"));
        assert_eq!(s.get("guest[4]_name"), None);
        assert_eq!(s.get("guests_summary"), Some("3/4"));
        assert_eq!(s.get("pve_status_ok"), Some("ONLINE"));
        assert_eq!(p.count(), 0);
    }

    #[test]
    fn running_guest_usage() {
        let (s, _) = run(&resources("pve-a"), 1, &[]);
        assert_eq!(s.get("guest[1]_status_ok"), Some("RUNNING"));
        assert_eq!(s.get("guest[1]_cpu_percent"), Some("12.5"));
        assert_eq!(s.get("guest[1]_mem_percent"), Some("50.0"));
        assert_eq!(s.get("guest[1]_mem"), Some("16.0 GB"));
        assert_eq!(s.get("guest[1]_uptime"), Some("3d 04h"));
        assert_eq!(s.get("guest[1]_cpus"), Some("4"));
    }

    #[test]
    fn stopped_guest_has_no_usage() {
        let (s, _) = run(&resources("pve-a"), 1, &[]);
        assert_eq!(s.get("guest[2]_status_warn"), Some("STOPPED"));
        assert_eq!(s.get("guest[2]_cpu_percent"), None);
    }

    #[test]
    fn expected_running_guest_is_a_problem() {
        let mut res = resources("pve-a");
        res.iter_mut()
            .filter(|r| r.vmid == Some(102))
            .for_each(|r| r.expect_running = true);
        let (s, p) = run(&res, 1, &[]);
        assert_eq!(s.get("guest[2]_status_bad"), Some("STOPPED"));
        assert_eq!(p.count(), 1);
    }

    #[test]
    fn node_and_storage() {
        let (s, _) = run(&resources("pve-a"), 1, &[]);
        assert_eq!(s.get("pve_node[0]_name"), Some("pve-a"));
        assert_eq!(s.get("pve_node[0]_status_ok"), Some("ONLINE"));
        assert_eq!(s.get("pve_node[0]_cpu_percent"), Some("7.0"));
        assert_eq!(s.get("pve_node[0]_guests"), Some("3/4"));
        assert_eq!(s.get("pve_storage[0]_name"), Some("local"));
        assert_eq!(s.get("pve_storage[1]_name"), Some("local-zfs"));
        assert_eq!(s.get("pve_storage[1]_used_percent"), Some("25.0"));
        // unavailable storage is skipped
        assert_eq!(s.get("pve_storages_total"), Some("2"));
    }

    #[test]
    fn standalone_hosts_are_merged() {
        let mut all = resources("pve-a");
        all.extend(resources("pve-b"));
        let (s, p) = run(&all, 3, &["pve-c"]);
        // same vmids on both hosts: sorted by node, then id
        assert_eq!(s.get("guest[0]_tag"), Some("VM pve-a"));
        assert_eq!(s.get("guest[4]_name"), Some("opnsense"));
        assert_eq!(s.get("guest[4]_tag"), Some("VM pve-b"));
        assert_eq!(s.get("guests_summary"), Some("6/8"));
        assert_eq!(s.get("pve_node[1]_name"), Some("pve-b"));
        assert_eq!(s.get("pve_node[2]_name"), Some("pve-c"));
        assert_eq!(s.get("pve_node[2]_status_bad"), Some("UNREACHABLE"));
        assert_eq!(s.get("pve_node[2]_guests"), None);
        assert_eq!(s.get("pve_nodes_online"), Some("2"));
        assert_eq!(s.get("pve_status_warn"), Some("2/3 UP"));
        assert_eq!(s.get("pve_storages_total"), Some("4"));
        assert_eq!(p.count(), 1);
    }

    #[test]
    fn all_hosts_down() {
        let (s, p) = run(&[], 1, &["pve-a"]);
        assert_eq!(s.get("pve_reachable"), Some("0"));
        assert_eq!(s.get("pve_status_bad"), Some("OFFLINE"));
        assert_eq!(s.get("pve_node[0]_status_bad"), Some("UNREACHABLE"));
        assert_eq!(p.count(), 1);
    }

    #[test]
    fn guest_backup_age() {
        let (s, p) = run(&with_backups(resources("pve-a")), 1, &[]);
        // 100 opnsense: fresh
        assert_eq!(s.get("guest[0]_backup_ok"), Some("5h"));
        assert_eq!(s.get("guest[0]_backup_age_hours"), Some("5"));
        // 101 truenas: 3 days old
        assert_eq!(s.get("guest[1]_backup_warn"), Some("3d"));
        // 102 homeassistant: no backup, informational only
        assert_eq!(s.get("guest[2]_backup_none"), Some("NONE"));
        assert_eq!(s.get("guest[2]_backup_warn"), None);
        // 200 pihole: ignored
        assert_eq!(s.get("guest[3]_backup"), None);

        assert_eq!(s.get("backups_summary_warn"), Some("1/2"));
        assert_eq!(s.get("backups_none"), Some("1"));
        assert_eq!(s.get("backups_oldest_name"), Some("truenas"));
        assert_eq!(s.get("backups_oldest_age"), Some("3d"));
        assert_eq!(p.count(), 1);
    }

    #[test]
    fn failed_guest_backup() {
        let mut res = resources("pve-a");
        for r in res.iter_mut() {
            r.backup_max_age = 26 * HOUR;
            r.backup = match r.vmid {
                // failed after an earlier successful backup
                Some(100) => Backup::Known(GuestBackup {
                    last_ok: Some(NOW - 30 * HOUR),
                    last_failed: true,
                }),
                // never succeeded
                Some(101) => Backup::Known(GuestBackup {
                    last_ok: None,
                    last_failed: true,
                }),
                _ => Backup::Ignored,
            };
        }
        let (s, p) = run(&res, 1, &[]);
        assert_eq!(s.get("guest[0]_backup_bad"), Some("FAILED"));
        assert_eq!(s.get("guest[0]_backup_age_hours"), Some("30"));
        assert_eq!(s.get("guest[1]_backup_bad"), Some("FAILED"));
        assert_eq!(s.get("backups_summary_warn"), Some("0/2"));
        // one problem per failed guest, not for the stale age as well
        assert_eq!(p.count(), 2);
    }

    #[test]
    fn vzdump_log_parsing() {
        let log = [
            "INFO: starting new backup job: vzdump 100 101 103 --mode snapshot --storage pbs",
            "INFO: Starting Backup of VM 100 (qemu)",
            "INFO: Finished Backup of VM 100 (00:01:23)",
            "INFO: Starting Backup of VM 101 (lxc)",
            "ERROR: Backup of VM 101 failed - unable to create temporary directory",
            "INFO: Starting Backup of VM 103 (qemu)",
            "INFO:  60% (12.0 GiB of 20.0 GiB) in 30s",
            "INFO: Finished Backup of VM 103 (00:00:42)",
            "INFO: Backup job finished with errors",
            "TASK ERROR: job errors",
        ];
        assert_eq!(
            parse_vzdump_log(log.into_iter()),
            vec![(100, true), (101, false), (103, true)]
        );
    }

    #[test]
    fn job_failure_with_known_guests_is_not_counted_twice() {
        let mut failed = job("pve-a", Some("job errors"), 1);
        failed.failed_guests = 1;
        let mut sensors = Sensors::new();
        let mut problems = Problems::default();
        write_backup_jobs(&[failed], NOW, &mut sensors, &mut problems);
        assert_eq!(sensors.get("pve_backup_job[0]_state_bad"), Some("FAILED"));
        assert_eq!(sensors.get("pve_backup_job[0]_failed_guests"), Some("1"));
        assert_eq!(problems.count(), 0);
    }

    #[test]
    fn unknown_backup_status_has_no_summary() {
        let (s, _) = run(&resources("pve-a"), 1, &[]);
        assert_eq!(s.get("guest[0]_backup"), None);
        assert_eq!(s.get("backups_summary"), None);
    }

    #[test]
    fn backup_jobs() {
        let jobs = [
            job("pve-b", Some("OK"), 6),
            job("pve-a", Some("WARNINGS: 1"), 30),
            job("pve-c", Some("job errors"), 2),
            job("pve4", None, 0),
        ];
        let mut sensors = Sensors::new();
        let mut problems = Problems::default();
        write_backup_jobs(&jobs, NOW, &mut sensors, &mut problems);
        assert_eq!(sensors.get("pve_backup_job[0]_node"), Some("pve-a"));
        assert_eq!(
            sensors.get("pve_backup_job[0]_state_warn"),
            Some("WARNINGS")
        );
        assert_eq!(sensors.get("pve_backup_job[0]_age"), Some("30h"));
        assert_eq!(sensors.get("pve_backup_job[1]_state_ok"), Some("OK"));
        assert_eq!(sensors.get("pve_backup_job[2]_state_bad"), Some("FAILED"));
        assert_eq!(sensors.get("pve_backup_job[2]_error"), Some("job errors"));
        assert_eq!(sensors.get("pve_backup_job[3]_state_ok"), Some("RUNNING"));
        assert_eq!(problems.count(), 2);
    }

    #[test]
    fn host_name_from_url() {
        assert_eq!(host_of("https://127.0.0.1:8006"), "127.0.0.1");
        assert_eq!(host_of("https://pve-b.lan:8006/"), "pve-b.lan");
        assert_eq!(host_of("https://pve-c"), "pve-c");
    }
}
