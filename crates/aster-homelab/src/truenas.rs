// SPDX-License-Identifier: MIT OR Apache-2.0

//! TrueNAS data source using the JSON-RPC 2.0 WebSocket API (`wss://<host>/api/current`).
//!
//! The REST API is deprecated since TrueNAS 25.04. One authenticated connection is kept open and
//! re-established on errors, to avoid an audit log entry for every poll.
//!
//! API keys must only be used over TLS: TrueNAS revokes keys that were sent over plain HTTP!

use crate::config::TrueNasConfig;
use crate::sensors::{
    Level, Problems, Sensors, epoch_secs, format_age, format_bytes, format_uptime, percent,
    truncate,
};
use crate::tls::{TlsTrust, client_config};
use anyhow::{Context, anyhow, bail};
use log::{debug, info};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Connector, Message, WebSocket};

const TIMEOUT: Duration = Duration::from_secs(10);
const NAME_LEN: usize = 14;
const MAX_ALERT_TEXTS: usize = 3;

pub struct TrueNas {
    host: String,
    port: u16,
    username: Option<String>,
    api_key: String,
    tls: Arc<rustls::ClientConfig>,
    socket: Option<WebSocket<MaybeTlsStream<TcpStream>>>,
    next_id: u64,
    exclude_disks: HashSet<String>,
    hdd_limits: (f64, f64),
    ssd_limits: (f64, f64),
    pool_limits: (f64, f64),
    scrub_max_age_days: f64,
    task_max_age_days: f64,
    exclude_tasks: HashSet<String>,
}

/// Data retrieved in one poll.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub info: Value,
    pub pools: Value,
    pub disks: Option<Value>,
    pub temperatures: Option<Value>,
    pub alerts: Option<Value>,
    pub apps: Option<Value>,
    pub replication: Option<Value>,
    pub cloudsync: Option<Value>,
    pub rsync: Option<Value>,
    pub snapshot_tasks: Option<Value>,
}

impl TrueNas {
    pub fn new(cfg: &TrueNasConfig, config_dir: &Path) -> anyhow::Result<Self> {
        let api_key = crate::config::secret(
            "api_key",
            cfg.api_key.as_deref(),
            cfg.api_key_file.as_deref(),
            config_dir,
        )?;
        let (host, port) = split_host_port(&cfg.host)?;
        let trust = TlsTrust::new(cfg.tls_fingerprint.as_deref(), cfg.tls_insecure)?;

        Ok(Self {
            host,
            port,
            username: cfg.username.clone(),
            api_key,
            tls: client_config(&trust)?,
            socket: None,
            next_id: 1,
            exclude_disks: cfg.exclude_disks.iter().cloned().collect(),
            hdd_limits: cfg.hdd_temp_limits.unwrap_or((45.0, 55.0)),
            ssd_limits: cfg.ssd_temp_limits.unwrap_or((60.0, 70.0)),
            pool_limits: cfg.pool_usage_limits.unwrap_or((80.0, 90.0)),
            scrub_max_age_days: cfg.scrub_max_age_days.unwrap_or(40.0),
            task_max_age_days: cfg.task_max_age_days.unwrap_or(8.0),
            exclude_tasks: cfg.exclude_tasks.iter().cloned().collect(),
        })
    }

    fn connect(&mut self) -> anyhow::Result<()> {
        let addr = (self.host.as_str(), self.port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| anyhow!("Cannot resolve {}", self.host))?;
        let tcp = TcpStream::connect_timeout(&addr, TIMEOUT)
            .with_context(|| format!("Failed to connect to {addr}"))?;
        tcp.set_read_timeout(Some(TIMEOUT))?;
        tcp.set_write_timeout(Some(TIMEOUT))?;

        let host = if self.host.contains(':') {
            format!("[{}]", self.host) // IPv6
        } else {
            self.host.clone()
        };
        let url = format!("wss://{host}:{}/api/current", self.port);
        let (socket, _) = tungstenite::client_tls_with_config(
            url.as_str(),
            tcp,
            None,
            Some(Connector::Rustls(self.tls.clone())),
        )
        .map_err(|e| anyhow!("WebSocket connection to {url} failed: {e}"))?;
        self.socket = Some(socket);

        let authenticated = match &self.username {
            Some(username) => {
                let result = self.call(
                    "auth.login_ex",
                    json!([{
                        "mechanism": "API_KEY_PLAIN",
                        "username": username,
                        "api_key": self.api_key,
                    }]),
                )?;
                result.get("response_type").and_then(Value::as_str) == Some("SUCCESS")
            }
            None => self
                .call("auth.login_with_api_key", json!([self.api_key]))?
                .as_bool()
                .unwrap_or_default(),
        };
        if !authenticated {
            self.socket = None;
            bail!("TrueNAS authentication failed");
        }

        info!("Connected to TrueNAS {url}");
        Ok(())
    }

    fn call(&mut self, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let socket = self
            .socket
            .as_mut()
            .ok_or_else(|| anyhow!("Not connected"))?;
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        socket.send(Message::Text(request.to_string().into()))?;

        loop {
            match socket.read()? {
                Message::Text(text) => {
                    let response: Value = serde_json::from_str(text.as_str())?;
                    if response.get("id").and_then(Value::as_u64) != Some(id) {
                        continue; // notification or a late response
                    }
                    if let Some(error) = response.get("error") {
                        let message = error
                            .pointer("/data/reason")
                            .or_else(|| error.get("message"))
                            .and_then(Value::as_str)
                            .unwrap_or("unknown error");
                        bail!("{method}: {message}");
                    }
                    return Ok(response.get("result").cloned().unwrap_or(Value::Null));
                }
                Message::Close(frame) => {
                    self.socket = None;
                    bail!("Connection closed by TrueNAS: {frame:?}");
                }
                _ => {}
            }
        }
    }

    /// Optional call: failures are logged, for example if apps are not configured.
    fn call_optional(&mut self, method: &str, params: Value) -> anyhow::Result<Option<Value>> {
        match self.call(method, params) {
            Ok(value) => Ok(Some(value)),
            Err(e) if self.socket.is_some() => {
                debug!("Ignoring failed optional call: {e}");
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    pub fn fetch(&mut self) -> anyhow::Result<Snapshot> {
        if self.socket.is_none() {
            self.connect()?;
        }
        let result = (|| {
            Ok(Snapshot {
                info: self.call("system.info", json!([]))?,
                pools: self.call("pool.query", json!([]))?,
                disks: self.call_optional("disk.query", json!([]))?,
                temperatures: self.call_optional("disk.temperatures", json!([]))?,
                alerts: self.call_optional("alert.list", json!([]))?,
                apps: self.call_optional("app.query", json!([]))?,
                replication: self.call_optional("replication.query", json!([]))?,
                cloudsync: self.call_optional("cloudsync.query", json!([]))?,
                rsync: self.call_optional("rsynctask.query", json!([]))?,
                snapshot_tasks: self.call_optional("pool.snapshottask.query", json!([]))?,
            })
        })();
        if result.is_err() {
            // reconnect on next poll
            self.socket = None;
        }
        result
    }

    pub fn update(&mut self, sensors: &mut Sensors, problems: &mut Problems) -> anyhow::Result<()> {
        match self.fetch() {
            Ok(snapshot) => {
                let limits = Limits {
                    exclude_disks: &self.exclude_disks,
                    hdd: self.hdd_limits,
                    ssd: self.ssd_limits,
                    pool: self.pool_limits,
                    scrub_max_age_days: self.scrub_max_age_days,
                    task_max_age_days: self.task_max_age_days,
                    exclude_tasks: &self.exclude_tasks,
                    now: chrono::Utc::now().timestamp(),
                };
                write_sensors(&snapshot, &limits, sensors, problems);
                Ok(())
            }
            Err(e) => {
                write_offline(sensors, problems);
                Err(e)
            }
        }
    }
}

/// Sensor values for an unreachable or misconfigured TrueNAS.
pub fn write_offline(sensors: &mut Sensors, problems: &mut Problems) {
    sensors.put("truenas_reachable", 0);
    sensors.put_state("truenas_status", "OFFLINE", Level::Bad);
    problems.add(Level::Bad, "TRUENAS UNREACHABLE");
}

fn split_host_port(host: &str) -> anyhow::Result<(String, u16)> {
    let host = host
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("wss://")
        .trim_end_matches('/');
    if let Some(rest) = host.strip_prefix('[') {
        // [IPv6]:port
        let (addr, port) = rest
            .split_once(']')
            .ok_or_else(|| anyhow!("Invalid host {host}"))?;
        let port = port
            .strip_prefix(':')
            .map(str::parse)
            .transpose()?
            .unwrap_or(443);
        return Ok((addr.to_string(), port));
    }
    match host.rsplit_once(':') {
        Some((name, port)) if !name.contains(':') => Ok((name.to_string(), port.parse()?)),
        _ => Ok((host.to_string(), 443)),
    }
}

pub struct Limits<'a> {
    pub exclude_disks: &'a HashSet<String>,
    pub hdd: (f64, f64),
    pub ssd: (f64, f64),
    /// Pool usage warning / critical thresholds in percent
    pub pool: (f64, f64),
    pub scrub_max_age_days: f64,
    pub task_max_age_days: f64,
    pub exclude_tasks: &'a HashSet<String>,
    /// Current time as Unix timestamp
    pub now: i64,
}

/// Convert a TrueNAS snapshot into sensor values.
pub fn write_sensors(
    snapshot: &Snapshot,
    limits: &Limits,
    sensors: &mut Sensors,
    problems: &mut Problems,
) {
    sensors.put("truenas_reachable", 1);
    sensors.put_state("truenas_status", "ONLINE", Level::Ok);

    let info = &snapshot.info;
    if let Some(hostname) = info.get("hostname").and_then(Value::as_str) {
        sensors.put("truenas_hostname", hostname);
    }
    if let Some(version) = info.get("version").and_then(Value::as_str) {
        sensors.put("truenas_version", version.trim_start_matches("TrueNAS-"));
    }
    if let Some(uptime) = info.get("uptime_seconds").and_then(Value::as_f64) {
        sensors.put("truenas_uptime", format_uptime(uptime as u64));
    }

    // pools
    let mut pools: Vec<&Value> = snapshot
        .pools
        .as_array()
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    pools.sort_by_key(|p| {
        p.get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    });
    let mut healthy = 0;
    for (idx, pool) in pools.iter().enumerate() {
        let key = format!("pool[{idx}]");
        let name = pool.get("name").and_then(Value::as_str).unwrap_or("?");
        let status = pool
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("UNKNOWN");
        let is_healthy = pool
            .get("healthy")
            .and_then(Value::as_bool)
            .unwrap_or(status == "ONLINE");
        sensors.put(format!("{key}_name"), truncate(name, NAME_LEN));
        if is_healthy && status == "ONLINE" {
            healthy += 1;
            sensors.put_state(&format!("{key}_status"), status, Level::Ok);
        } else {
            sensors.put_state(&format!("{key}_status"), status, Level::Bad);
            problems.add(
                Level::Bad,
                format!("POOL {} {status}", truncate(name, NAME_LEN).to_uppercase()),
            );
        }

        let size = pool.get("size").and_then(Value::as_u64);
        let allocated = pool.get("allocated").and_then(Value::as_u64);
        let free = pool.get("free").and_then(Value::as_u64);
        if let (Some(size), Some(allocated)) = (size, allocated) {
            let used = percent(allocated, size);
            sensors.put(format!("{key}_used_percent"), format!("{used:.1}"));
            sensors.put(format!("{key}_used"), format_bytes(allocated));
            sensors.put(format!("{key}_size"), format_bytes(size));
            let (warn, crit) = limits.pool;
            let level = if used >= crit {
                Level::Bad
            } else if used >= warn {
                Level::Warn
            } else {
                Level::Ok
            };
            sensors.put_state(&format!("{key}_used_level"), format!("{used:.0}%"), level);
            if level != Level::Ok {
                problems.add(
                    level,
                    format!(
                        "POOL {} {used:.0}% FULL",
                        truncate(name, NAME_LEN).to_uppercase()
                    ),
                );
            }
        }
        if let Some(free) = free {
            sensors.put(format!("{key}_free"), format_bytes(free));
        }
        write_scrub(pool, &key, name, limits, sensors, problems);
    }
    sensors.put("pools_total", pools.len());
    sensors.put("pools_healthy", healthy);
    sensors.put("pools_summary", format!("{healthy}/{}", pools.len()));

    write_disks(snapshot, limits, sensors, problems);
    write_alerts(snapshot, sensors, problems);
    write_apps(snapshot, sensors, problems);
    write_tasks(snapshot, limits, sensors, problems);
}

/// Scrub or resilver status of a pool from the `scan` object of `pool.query`.
fn write_scrub(
    pool: &Value,
    key: &str,
    name: &str,
    limits: &Limits,
    sensors: &mut Sensors,
    problems: &mut Problems,
) {
    let key = format!("{key}_scrub");
    let pool_name = truncate(name, NAME_LEN).to_uppercase();
    let Some(scan) = pool.get("scan").filter(|s| s.is_object()) else {
        sensors.put_state(&key, "NEVER", Level::Warn);
        return;
    };
    let function = scan
        .get("function")
        .and_then(Value::as_str)
        .unwrap_or("SCRUB");
    let state = scan
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let errors = scan
        .get("errors")
        .and_then(Value::as_u64)
        .unwrap_or_default();

    if state == "SCANNING" {
        let pct = scan
            .get("percentage")
            .and_then(Value::as_f64)
            .unwrap_or_default();
        let paused = scan.get("pause").is_some_and(|p| !p.is_null());
        let text = format!(
            "{} {pct:.0}%{}",
            function,
            if paused { " PAUSED" } else { "" }
        );
        let level = if function == "RESILVER" {
            Level::Warn
        } else {
            Level::Ok
        };
        sensors.put_state(&key, text, level);
        sensors.put(format!("{key}_percent"), format!("{pct:.1}"));
        if let Some(left) = scan.get("total_secs_left").and_then(Value::as_i64) {
            sensors.put(format!("{key}_eta"), format!("{} left", format_age(left)));
        }
        if function == "RESILVER" {
            problems.add(Level::Warn, format!("POOL {pool_name} RESILVERING"));
        }
        return;
    }

    let finished = scan.get("end_time").and_then(epoch_secs);
    let age = finished.map(|t| limits.now - t);
    if let Some(age) = age {
        sensors.put(format!("{key}_age_days"), age / 86_400);
        sensors.put(format!("{key}_age"), format_age(age));
    }
    sensors.put(format!("{key}_errors"), errors);

    if errors > 0 {
        sensors.put_state(&key, format!("{errors} ERRORS"), Level::Bad);
        problems.add(Level::Bad, format!("POOL {pool_name} SCRUB ERRORS"));
    } else if state == "CANCELED" {
        sensors.put_state(&key, "CANCELED", Level::Warn);
        problems.add(Level::Warn, format!("POOL {pool_name} SCRUB CANCELED"));
    } else if let Some(age) = age {
        let text = format!("{} AGO", format_age(age).to_uppercase());
        if age as f64 > limits.scrub_max_age_days * 86_400.0 {
            sensors.put_state(&key, text, Level::Warn);
            problems.add(Level::Warn, format!("POOL {pool_name} SCRUB OVERDUE"));
        } else {
            sensors.put_state(&key, text, Level::Ok);
        }
    } else {
        sensors.put_state(&key, "NEVER", Level::Warn);
    }
}

/// Status of a data protection task.
#[derive(Debug, PartialEq)]
struct TaskStatus {
    kind: &'static str,
    name: String,
    /// Task state, `None` if it never ran
    state: Option<String>,
    /// Last run (finished or started) as Unix timestamp
    last: Option<i64>,
}

/// Parse a replication, cloud sync, rsync or periodic snapshot task.
///
/// Tasks report their last run either in a `state` object (`state`, `datetime`) or in a `job`
/// object (`state`, `time_finished`). Disabled tasks are skipped.
fn parse_task(kind: &'static str, task: &Value) -> Option<TaskStatus> {
    if task.get("enabled").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let name = ["name", "description", "dataset", "path"]
        .iter()
        .filter_map(|f| task.get(*f).and_then(Value::as_str))
        .find(|n| !n.trim().is_empty())
        .unwrap_or("?")
        .to_string();

    let job = task.get("job").filter(|j| j.is_object());
    let state_obj = task.get("state").filter(|s| s.is_object());
    let (state, last) = if let Some(job) = job {
        (
            job.get("state").and_then(Value::as_str),
            job.get("time_finished")
                .and_then(epoch_secs)
                .or_else(|| job.get("time_started").and_then(epoch_secs)),
        )
    } else if let Some(state) = state_obj {
        (
            state.get("state").and_then(Value::as_str),
            state.get("datetime").and_then(epoch_secs),
        )
    } else {
        (task.get("state").and_then(Value::as_str), None)
    };

    Some(TaskStatus {
        kind,
        name,
        state: state.map(str::to_uppercase),
        last,
    })
}

fn write_tasks(
    snapshot: &Snapshot,
    limits: &Limits,
    sensors: &mut Sensors,
    problems: &mut Problems,
) {
    let sources = [
        ("REPL", &snapshot.replication),
        ("CLOUD", &snapshot.cloudsync),
        ("RSYNC", &snapshot.rsync),
        ("SNAP", &snapshot.snapshot_tasks),
    ];
    let mut tasks: Vec<TaskStatus> = Vec::new();
    for (kind, list) in sources {
        let Some(list) = list.as_ref().and_then(Value::as_array) else {
            continue;
        };
        let mut parsed: Vec<TaskStatus> = list
            .iter()
            .filter_map(|t| parse_task(kind, t))
            .filter(|t| !limits.exclude_tasks.contains(&t.name))
            .collect();
        parsed.sort_by(|a, b| a.name.cmp(&b.name));
        tasks.extend(parsed);
    }

    struct Row {
        kind: &'static str,
        name: String,
        age: Option<i64>,
        text: String,
        level: Level,
    }

    let mut rows: Vec<Row> = Vec::new();
    // healthy periodic snapshot tasks are combined into one row: there may be dozens
    let mut snap_ok: Vec<Option<i64>> = Vec::new();
    for task in &tasks {
        let name = truncate(&task.name, 24);
        let age = task.last.map(|t| limits.now - t);
        let state = task.state.as_deref().unwrap_or("NEVER RUN");
        let stale = age.is_some_and(|a| a as f64 > limits.task_max_age_days * 86_400.0);
        let level = match state {
            "ERROR" | "FAILED" | "ABORTED" => {
                problems.add(
                    Level::Bad,
                    format!("{} {} FAILED", task.kind, name.to_uppercase()),
                );
                Level::Bad
            }
            "RUNNING" | "WAITING" | "PENDING" => Level::Ok,
            "NEVER RUN" => Level::Warn,
            _ if stale => {
                problems.add(
                    Level::Warn,
                    format!("{} {} STALE", task.kind, name.to_uppercase()),
                );
                Level::Warn
            }
            _ => Level::Ok,
        };
        if task.kind == "SNAP" && level == Level::Ok {
            snap_ok.push(age);
            continue;
        }
        let text = match state {
            "SUCCESS" | "FINISHED" => "OK",
            other => other,
        };
        rows.push(Row {
            kind: task.kind,
            name,
            age,
            text: text.to_string(),
            level,
        });
    }
    if !snap_ok.is_empty() {
        let count = snap_ok.len();
        rows.push(Row {
            kind: "SNAP",
            name: format!("{count} snapshot task{}", if count > 1 { "s" } else { "" }),
            // the least recent run of all combined tasks
            age: snap_ok.iter().flatten().copied().max(),
            text: "OK".to_string(),
            level: Level::Ok,
        });
    }

    for (idx, row) in rows.iter().enumerate() {
        let key = format!("task[{idx}]");
        sensors.put(format!("{key}_name"), &row.name);
        sensors.put(format!("{key}_kind"), row.kind);
        if let Some(age) = row.age {
            sensors.put(format!("{key}_age"), format_age(age));
        }
        sensors.put_state(&format!("{key}_state"), &row.text, row.level);
    }

    let not_ok: Vec<Level> = rows
        .iter()
        .map(|r| r.level)
        .filter(|l| *l != Level::Ok)
        .collect();
    let ok = tasks.len() - not_ok.len();
    let level = not_ok.iter().copied().max().unwrap_or(Level::Ok);
    sensors.put("tasks_total", tasks.len());
    sensors.put("tasks_ok", ok);
    sensors.put_state("tasks_summary", format!("{ok}/{}", tasks.len()), level);
}

fn write_disks(
    snapshot: &Snapshot,
    limits: &Limits,
    sensors: &mut Sensors,
    problems: &mut Problems,
) {
    let temps = snapshot.temperatures.as_ref().and_then(Value::as_object);
    let mut disks: Vec<(String, String, Option<f64>)> = Vec::new(); // name, type, temperature
    if let Some(list) = snapshot.disks.as_ref().and_then(Value::as_array) {
        for disk in list {
            let Some(name) = disk.get("name").and_then(Value::as_str) else {
                continue;
            };
            let model = disk
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or_default();
            // skip virtual disks, for example the boot disk of a TrueNAS VM
            if limits.exclude_disks.contains(name)
                || name.starts_with("vd")
                || model.to_uppercase().contains("QEMU")
            {
                continue;
            }
            let kind = disk
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("HDD")
                .to_string();
            let temp = temps.and_then(|t| t.get(name)).and_then(temperature);
            disks.push((name.to_string(), kind, temp));
        }
    } else if let Some(temps) = temps {
        for (name, value) in temps {
            if !limits.exclude_disks.contains(name) {
                let kind = if name.starts_with("nvme") {
                    "SSD"
                } else {
                    "HDD"
                };
                disks.push((name.clone(), kind.to_string(), temperature(value)));
            }
        }
    }
    disks.sort_by(|a, b| natural_key(&a.0).cmp(&natural_key(&b.0)));

    let mut max_temp: Option<f64> = None;
    for (idx, (name, kind, temp)) in disks.iter().enumerate() {
        let key = format!("disk[{idx}]");
        sensors.put(format!("{key}_name"), name);
        sensors.put(format!("{key}_type"), kind);
        let Some(temp) = temp else {
            continue; // in standby or no sensor: don't wake it up
        };
        max_temp = Some(max_temp.map_or(*temp, |m: f64| m.max(*temp)));
        let (warn, crit) = if kind == "HDD" {
            limits.hdd
        } else {
            limits.ssd
        };
        let level = if *temp >= crit {
            Level::Bad
        } else if *temp >= warn {
            Level::Warn
        } else {
            Level::Ok
        };
        sensors.put(format!("{key}_temp"), format!("{temp:.0}"));
        sensors.put_state(&format!("{key}_temp_level"), format!("{temp:.0}°"), level);
        if level != Level::Ok {
            problems.add(level, format!("DISK {name} {temp:.0}°C"));
        }
    }
    sensors.put("disks_total", disks.len());
    if let Some(max) = max_temp {
        sensors.put("disks_max_temp", format!("{max:.0}"));
    }
}

/// Disk temperature: a plain number, or an object with a `temp` field (newer releases).
fn temperature(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.get("temp").and_then(Value::as_f64))
}

/// Sort `sdb` before `sdaa` and `nvme2n1` before `nvme10n1`.
fn natural_key(name: &str) -> (usize, String) {
    (name.len(), name.to_string())
}

fn write_alerts(snapshot: &Snapshot, sensors: &mut Sensors, problems: &mut Problems) {
    let Some(alerts) = snapshot.alerts.as_ref().and_then(Value::as_array) else {
        return;
    };

    let mut active: Vec<(u8, String)> = alerts
        .iter()
        .filter(|a| {
            !a.get("dismissed")
                .and_then(Value::as_bool)
                .unwrap_or_default()
        })
        .filter_map(|a| {
            let level = a.get("level").and_then(Value::as_str).unwrap_or("INFO");
            let severity = match level {
                "EMERGENCY" | "ALERT" | "CRITICAL" | "ERROR" => 2,
                "WARNING" => 1,
                _ => return None, // INFO, NOTICE
            };
            let text = a
                .get("formatted")
                .or_else(|| a.get("text"))
                .and_then(Value::as_str)
                .unwrap_or(level);
            Some((severity, text.to_string()))
        })
        .collect();
    active.sort_by(|a, b| b.0.cmp(&a.0));

    let critical = active.iter().filter(|a| a.0 == 2).count();
    let warning = active.len() - critical;
    sensors.put("truenas_alerts_critical", critical);
    sensors.put("truenas_alerts_warning", warning);
    let level = if critical > 0 {
        Level::Bad
    } else if warning > 0 {
        Level::Warn
    } else {
        Level::Ok
    };
    sensors.put_state("truenas_alerts", active.len(), level);

    for (idx, (severity, text)) in active.iter().take(MAX_ALERT_TEXTS).enumerate() {
        let level = if *severity == 2 {
            Level::Bad
        } else {
            Level::Warn
        };
        sensors.put_state(
            &format!("truenas_alert[{idx}]"),
            truncate(&strip_html(text), 60),
            level,
        );
    }
    if critical > 0 {
        problems.add(
            Level::Bad,
            format!(
                "{critical} CRITICAL ALERT{}",
                if critical > 1 { "S" } else { "" }
            ),
        );
    }
    if warning > 0 {
        problems.add(
            Level::Warn,
            format!(
                "{warning} NAS WARNING{}",
                if warning > 1 { "S" } else { "" }
            ),
        );
    }
}

/// Alert texts may contain HTML markup.
fn strip_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn write_apps(snapshot: &Snapshot, sensors: &mut Sensors, problems: &mut Problems) {
    let Some(apps) = snapshot.apps.as_ref().and_then(Value::as_array) else {
        return;
    };
    let mut apps: Vec<(&str, &str)> = apps
        .iter()
        .filter_map(|a| {
            Some((
                a.get("name").and_then(Value::as_str)?,
                a.get("state").and_then(Value::as_str).unwrap_or("UNKNOWN"),
            ))
        })
        .collect();
    apps.sort();

    let mut running = 0;
    for (idx, (name, state)) in apps.iter().enumerate() {
        let key = format!("app[{idx}]");
        let level = match *state {
            "RUNNING" => {
                running += 1;
                Level::Ok
            }
            "CRASHED" => {
                problems.add(
                    Level::Bad,
                    format!("APP {} CRASHED", truncate(name, NAME_LEN).to_uppercase()),
                );
                Level::Bad
            }
            _ => Level::Warn,
        };
        sensors.put(format!("{key}_name"), truncate(name, NAME_LEN));
        sensors.put_state(&format!("{key}_state"), state, level);
    }
    sensors.put("apps_total", apps.len());
    sensors.put("apps_running", running);
    sensors.put("apps_summary", format!("{running}/{}", apps.len()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        let v: Value =
            serde_json::from_str(include_str!("../tests/truenas_snapshot.json")).unwrap();
        Snapshot {
            info: v["info"].clone(),
            pools: v["pools"].clone(),
            disks: Some(v["disks"].clone()),
            temperatures: Some(v["temperatures"].clone()),
            alerts: Some(v["alerts"].clone()),
            apps: Some(v["apps"].clone()),
            replication: Some(v["replication"].clone()),
            cloudsync: Some(v["cloudsync"].clone()),
            rsync: Some(v["rsync"].clone()),
            snapshot_tasks: Some(v["snapshot_tasks"].clone()),
        }
    }

    const NOW: i64 = 1_763_200_000;

    fn limits<'a>(exclude: &'a HashSet<String>, pool: (f64, f64)) -> Limits<'a> {
        Limits {
            exclude_disks: exclude,
            hdd: (45.0, 55.0),
            ssd: (60.0, 70.0),
            pool,
            scrub_max_age_days: 40.0,
            task_max_age_days: 8.0,
            exclude_tasks: exclude,
            now: NOW,
        }
    }

    fn run(snapshot: &Snapshot) -> (Sensors, Problems) {
        let exclude = HashSet::new();
        let limits = limits(&exclude, (80.0, 90.0));
        let mut sensors = Sensors::new();
        let mut problems = Problems::default();
        write_sensors(snapshot, &limits, &mut sensors, &mut problems);
        (sensors, problems)
    }

    #[test]
    fn system_info() {
        let (s, _) = run(&snapshot());
        assert_eq!(s.get("truenas_hostname"), Some("nas"));
        assert_eq!(s.get("truenas_version"), Some("25.04.2"));
        assert_eq!(s.get("truenas_uptime"), Some("12d 01h"));
    }

    #[test]
    fn pools() {
        let (s, _) = run(&snapshot());
        assert_eq!(s.get("pool[0]_name"), Some("fast"));
        assert_eq!(s.get("pool[0]_status_ok"), Some("ONLINE"));
        assert_eq!(s.get("pool[1]_name"), Some("tank"));
        assert_eq!(s.get("pool[1]_status_bad"), Some("DEGRADED"));
        assert_eq!(s.get("pool[1]_used_percent"), Some("85.0"));
        assert_eq!(s.get("pool[1]_used_level_warn"), Some("85%"));
        assert_eq!(s.get("pools_summary"), Some("1/2"));
    }

    #[test]
    fn disks_sorted_without_virtual_disks() {
        let (s, _) = run(&snapshot());
        assert_eq!(s.get("disk[0]_name"), Some("sda"));
        assert_eq!(s.get("disk[1]_name"), Some("sdb"));
        assert_eq!(s.get("disk[2]_name"), Some("nvme0n1"));
        assert_eq!(s.get("disks_total"), Some("3"));
        assert_eq!(s.get("disk[0]_temp_level_ok"), Some("38°"));
        // standby disk: name but no temperature
        assert_eq!(s.get("disk[1]_temp"), None);
        assert_eq!(s.get("disk[2]_temp_level_warn"), Some("63°"));
        assert_eq!(s.get("disks_max_temp"), Some("63"));
    }

    #[test]
    fn alerts_and_apps() {
        let (s, _) = run(&snapshot());
        assert_eq!(s.get("truenas_alerts_critical"), Some("1"));
        assert_eq!(s.get("truenas_alerts_warning"), Some("1"));
        assert_eq!(
            s.get("truenas_alert[0]_bad"),
            Some("Pool tank state is DEGRADED")
        );
        assert_eq!(s.get("apps_summary"), Some("1/2"));
        assert_eq!(s.get("app[0]_name"), Some("immich"));
        assert_eq!(s.get("app[1]_state_warn"), Some("STOPPED"));
    }

    #[test]
    fn pool_usage_limits_are_configurable() {
        let exclude = HashSet::new();
        let limits = limits(&exclude, (90.0, 95.0));
        let mut sensors = Sensors::new();
        let mut problems = Problems::default();
        write_sensors(&snapshot(), &limits, &mut sensors, &mut problems);
        assert_eq!(sensors.get("pool[1]_used_level_ok"), Some("85%"));
        // degraded pool, nvme temperature, critical alert, warning alert, failed and stale task
        assert_eq!(problems.count(), 6);
    }

    #[test]
    fn problems_are_counted() {
        let (_, p) = run(&snapshot());
        // degraded pool, pool 85% full, nvme temperature, critical alert, warning alert,
        // failed cloud sync, stale replication
        assert_eq!(p.count(), 7);
    }

    #[test]
    fn scrub_status() {
        let (s, _) = run(&snapshot());
        // pool[0] = fast: scrub running
        assert_eq!(s.get("pool[0]_scrub_ok"), Some("SCRUB 42%"));
        assert_eq!(s.get("pool[0]_scrub_percent"), Some("42.5"));
        assert_eq!(s.get("pool[0]_scrub_eta"), Some("5h left"));
        // pool[1] = tank: finished 3 days ago
        assert_eq!(s.get("pool[1]_scrub_ok"), Some("3D AGO"));
        assert_eq!(s.get("pool[1]_scrub_age_days"), Some("3"));
        assert_eq!(s.get("pool[1]_scrub_errors"), Some("0"));
    }

    #[test]
    fn scrub_problems() {
        let exclude = HashSet::new();
        let limits = limits(&exclude, (80.0, 90.0));
        let mut pools: Value = serde_json::from_str(
            r#"[{"name": "a", "status": "ONLINE", "healthy": true,
                 "scan": {"function": "SCRUB", "state": "FINISHED", "end_time": {"$date": 1758000000000}, "errors": 0}},
                {"name": "b", "status": "ONLINE", "healthy": true,
                 "scan": {"function": "SCRUB", "state": "FINISHED", "end_time": {"$date": 1763100000000}, "errors": 4}},
                {"name": "c", "status": "ONLINE", "healthy": true,
                 "scan": {"function": "RESILVER", "state": "SCANNING", "percentage": 10.0}},
                {"name": "d", "status": "ONLINE", "healthy": true, "scan": null}]"#,
        )
        .unwrap();
        let snapshot = Snapshot {
            info: json!({}),
            pools: pools.take(),
            ..Default::default()
        };
        let mut s = Sensors::new();
        let mut p = Problems::default();
        write_sensors(&snapshot, &limits, &mut s, &mut p);
        assert_eq!(s.get("pool[0]_scrub_warn"), Some("60D AGO"));
        assert_eq!(s.get("pool[1]_scrub_bad"), Some("4 ERRORS"));
        assert_eq!(s.get("pool[2]_scrub_warn"), Some("RESILVER 10%"));
        assert_eq!(s.get("pool[3]_scrub_warn"), Some("NEVER"));
        // overdue, scrub errors, resilvering
        assert_eq!(p.count(), 3);
    }

    #[test]
    fn data_protection_tasks() {
        let (s, _) = run(&snapshot());
        // replication, cloud sync, rsync, snapshot tasks; sorted by name; disabled skipped
        assert_eq!(s.get("task[0]_name"), Some("old replication"));
        assert_eq!(s.get("task[0]_state_warn"), Some("OK"));
        assert_eq!(s.get("task[0]_age"), Some("10d"));
        assert_eq!(s.get("task[1]_name"), Some("tank -> backup"));
        assert_eq!(s.get("task[1]_kind"), Some("REPL"));
        assert_eq!(s.get("task[1]_state_ok"), Some("OK"));
        assert_eq!(s.get("task[1]_age"), Some("6h"));
        assert_eq!(s.get("task[2]_name"), Some("b2 photos"));
        assert_eq!(s.get("task[2]_kind"), Some("CLOUD"));
        assert_eq!(s.get("task[2]_state_bad"), Some("FAILED"));
        // healthy snapshot tasks combined, with the least recent run
        assert_eq!(s.get("task[3]_name"), Some("2 snapshot tasks"));
        assert_eq!(s.get("task[3]_kind"), Some("SNAP"));
        assert_eq!(s.get("task[3]_state_ok"), Some("OK"));
        assert_eq!(s.get("task[3]_age"), Some("1h"));
        assert_eq!(s.get("task[4]_name"), None);
        assert_eq!(s.get("tasks_summary_bad"), Some("3/5"));
    }

    #[test]
    fn task_without_runs() {
        let task = json!({"name": "new", "enabled": true, "state": {"state": null}});
        let parsed = parse_task("REPL", &task).unwrap();
        assert_eq!(parsed.state, None);
        assert_eq!(parsed.last, None);
        let task = json!({"description": "", "path": "/mnt/tank/x", "job": null});
        assert_eq!(parse_task("RSYNC", &task).unwrap().name, "/mnt/tank/x");
    }

    #[test]
    fn host_port_parsing() {
        assert_eq!(
            split_host_port("10.0.0.5").unwrap(),
            ("10.0.0.5".into(), 443)
        );
        assert_eq!(
            split_host_port("nas.lan:8443").unwrap(),
            ("nas.lan".into(), 8443)
        );
        assert_eq!(
            split_host_port("https://nas.lan/").unwrap(),
            ("nas.lan".into(), 443)
        );
        assert_eq!(
            split_host_port("[fd00::5]:444").unwrap(),
            ("fd00::5".into(), 444)
        );
        assert_eq!(split_host_port("fd00::5").unwrap(), ("fd00::5".into(), 443));
    }

    #[test]
    fn html_is_stripped() {
        assert_eq!(
            strip_html("Pool <b>tank</b>\n is  full"),
            "Pool tank is full"
        );
    }
}
