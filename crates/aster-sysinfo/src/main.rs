// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

#![forbid(non_ascii_idents)]
#![deny(unsafe_code)]

use clap::Parser;
use env_logger::Env;
use itertools::Itertools;
use log::{debug, error, info};
use regex::Regex;
use std::cmp::PartialEq;
use std::collections::HashMap;
use std::fmt::Display;
use std::fs;
use std::io::{BufWriter, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, exit};
use std::thread::sleep;
use std::time::{Duration, Instant};
use sysinfo::{Components, DiskKind, Disks, Networks, System};
use tempfile::Builder;

/// Proof of concept sensor value collection for the asterctl screen control tool.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Output sensor file.
    #[arg(short, long)]
    out: Option<PathBuf>,

    /// Temporary directory for preparing the output sensor file.
    ///
    /// The system temp directory is used if not specified.
    /// The temp directory must be on the same file system for atomic rename operation!
    #[arg(short, long)]
    temp_dir: Option<PathBuf>,

    /// Print values in console
    #[arg(long)]
    console: bool,

    /// System sensor refresh interval in seconds
    #[arg(short, long)]
    refresh: Option<u16>,

    /// Enable individual disk refresh logic as used in AOOSTAR-X. Refresh interval in seconds.
    #[arg(long)]
    disk_refresh: Option<u16>,

    /// Retrieve drive temperature if `disk-update` option is enabled.
    ///
    /// Requires smartctl and password-less sudo!
    #[cfg(target_os = "linux")]
    #[arg(long)]
    smartctl: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = Args::parse();
    #[cfg(target_os = "linux")]
    let use_smartctl = args.smartctl;
    #[cfg(not(target_os = "linux"))]
    let use_smartctl = false;

    if let Some(out_file) = &args.out
        && let Some(parent) = out_file.parent()
    {
        fs::create_dir_all(parent)?;
    }
    let mut sensors = HashMap::with_capacity(64);
    let mut sysinfo_source = SysinfoSource::new();

    let refresh = Duration::from_secs(args.refresh.unwrap_or_default() as u64);

    let disk_refresh = Duration::from_secs(args.disk_refresh.unwrap_or_default() as u64);
    let mut disk_refresh_time = Instant::now();
    if !disk_refresh.is_zero() {
        update_linux_storage_sensors(&mut sensors, use_smartctl)?;
    }

    if !refresh.is_zero() {
        info!(
            "Starting aster-sysinfo with refresh={}ms",
            refresh.as_millis()
        );
    }

    loop {
        let upd_start_time = Instant::now();

        sysinfo_source.refresh();
        sysinfo_source.update_sensors(&mut sensors)?;

        if !disk_refresh.is_zero() && disk_refresh_time.elapsed() > disk_refresh {
            debug!("Refreshing individual disks");
            update_linux_storage_sensors(&mut sensors, use_smartctl)?;
            disk_refresh_time = Instant::now();
        }

        if let Some(out_file) = &args.out {
            write_sensor_file(out_file, args.temp_dir.as_deref(), &sensors)?;
        }

        if args.console {
            // pretty print console output with sorted keys
            for (label, value) in sensors.iter().sorted() {
                println!("{}: {}", label, value);
            }
            println!();
        }

        if refresh.is_zero() {
            break;
        }

        let elapsed = upd_start_time.elapsed();
        if refresh > elapsed {
            sleep(refresh - elapsed);
        }
    }

    Ok(())
}

fn write_sensor_file(
    out_file: &Path,
    temp_dir: Option<&Path>,
    sensors: &HashMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if out_file.is_dir() {
        error!("Output cannot be a directory: {}", out_file.display());
        exit(1);
    }

    // make sure our sensor file can be read by everyone
    let all_read_perm = fs::Permissions::from_mode(0o664);
    let tmp_file = if let Some(temp_path) = temp_dir {
        fs::create_dir_all(temp_path)?;

        debug!("Creating a new named temp file in {temp_path:?}");
        Builder::new()
            .permissions(all_read_perm)
            .tempfile_in(temp_path)?
    } else {
        debug!("Creating a new named temp file");
        Builder::new().permissions(all_read_perm).tempfile()?
    };

    debug!("Writing sensor temp file...");
    let mut stream = BufWriter::new(&tmp_file);

    for (label, value) in sensors.iter() {
        writeln!(stream, "{label}: {value}")?;
    }

    stream.flush()?;
    drop(stream);
    debug!("Renaming temp file to: {out_file:?}");
    tmp_file.persist(out_file)?;

    Ok(())
}

pub struct SysinfoSource {
    sys: System,
    disks: Disks,
    components: Components,
    networks: Networks,
    last_refresh: Option<Instant>,
    refresh_duration: Option<Duration>,
}

impl Default for SysinfoSource {
    fn default() -> Self {
        Self::new()
    }
}

impl SysinfoSource {
    pub fn new() -> Self {
        Self {
            sys: System::new_all(),
            disks: Disks::new(),
            components: Components::new(),
            networks: Networks::new(),
            last_refresh: None,
            refresh_duration: None,
        }
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_all();
        debug!("Refreshing disks, components, networks");
        // TODO research "remove_not_listed_###" refresh parameter
        self.disks.refresh(false);
        self.components.refresh(false);
        self.networks.refresh(false);

        if let Some(last_refresh) = self.last_refresh {
            self.refresh_duration = Some(last_refresh.elapsed());
        }
        self.last_refresh = Some(Instant::now());
    }

    fn update_sensors(
        &self,
        sensors: &mut HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Refreshing sensors");
        for cpu in self.sys.cpus() {
            add_sensor(
                sensors,
                format!("cpu_{}_frequency", cpu.name()),
                cpu.frequency(),
            );
            add_sensor(
                sensors,
                format!("cpu_{}_usage", cpu.name()),
                format!("{:.2}", cpu.cpu_usage()),
            );
        }

        add_sensor(
            sensors,
            "cpu_usage_percent".to_string(),
            format!("{:.2}", self.sys.global_cpu_usage()),
        );

        let load_avg = System::load_average();
        add_sensor(sensors, "load_avg_one", format!("{:.2}", load_avg.one));
        add_sensor(sensors, "load_avg_five", format!("{:.2}", load_avg.five));
        add_sensor(
            sensors,
            "load_avg_fifteen",
            format!("{:.2}", load_avg.fifteen),
        );

        // RAM and swap information:
        add_sensor(sensors, "mem_free_bytes", self.sys.free_memory());
        add_sensor(sensors, "mem_free", format_bytes(self.sys.free_memory()));
        add_sensor(sensors, "mem_total_bytes", self.sys.total_memory());
        add_sensor(sensors, "mem_total", format_bytes(self.sys.total_memory()));
        add_sensor(sensors, "mem_used_bytes", self.sys.used_memory());
        add_sensor(sensors, "mem_used", format_bytes(self.sys.used_memory()));
        add_sensor(
            sensors,
            "mem_usage_percent",
            format!(
                "{:.1}",
                (self.sys.used_memory() * 100) as f64 / self.sys.total_memory() as f64
            ),
        );

        add_sensor(sensors, "swap_free_bytes", self.sys.free_swap());
        add_sensor(sensors, "swap_free", format_bytes(self.sys.free_swap()));
        add_sensor(sensors, "swap_total_bytes", self.sys.total_swap());
        add_sensor(sensors, "swap_total", format_bytes(self.sys.total_swap()));
        add_sensor(sensors, "swap_used_bytes", self.sys.used_swap());
        add_sensor(sensors, "swap_used", format_bytes(self.sys.used_swap()));
        add_sensor(
            sensors,
            "swap_usage_percent",
            format!(
                "{:.1}",
                (self.sys.used_swap() * 100) as f64 / self.sys.total_swap() as f64
            ),
        );

        // System information:
        let up_secs = System::uptime();
        let up_days = up_secs / 86400;
        let up_hours = (up_secs - (up_days * 86400)) / 3600;
        let up_mins = (up_secs - (up_days * 86400) - (up_hours * 3600)) / 60;
        add_sensor(sensors, "system_uptime_sec", up_secs);
        /*
        Time to look into ftl for i18n
        The coreutils project did a lot of work that could be used:
        https://github.com/uutils/coreutils/blob/main/src/uucore/src/lib/mods/locale.rs
        Then this would be the easy way to format the time, just uses a lot of setup code:

        uptime-format = { $days ->
            [0] { $time }
            [one] { $days } day, { $time }
           *[other] { $days } days { $time }
        }

        translate!(
            "uptime-format",
            "days" => up_days,
            "time" => format!("{up_hours:02}:{up_mins:02}")
        )
         */
        let day_string = match up_days {
            0 => "",
            1 => "1 day, ",
            n => &format!("{n} days "),
        };
        add_sensor(
            sensors,
            "system_uptime",
            format!("{day_string}{up_hours:02}:{up_mins:02}"),
        );

        if let Some(name) = System::name() {
            add_sensor(sensors, "system_name", name);
        }
        if let Some(kernel_version) = System::kernel_version() {
            add_sensor(sensors, "system_kernel_version", kernel_version);
        }
        if let Some(os_version) = System::os_version() {
            add_sensor(sensors, "system_os_version", os_version);
        }
        if let Some(host_name) = System::host_name() {
            add_sensor(sensors, "system_hostname", host_name);
        }

        add_sensor(sensors, "cpu_count", self.sys.cpus().len());
        add_sensor(sensors, "total_processes", self.sys.processes().len());

        // disks' information:
        let mut ssd_idx = 0;
        let mut hdd_idx = 0;
        for disk in &self.disks {
            let label;
            match disk.kind() {
                DiskKind::SSD => {
                    label = format!("storage_ssd[{}]", ssd_idx);
                    ssd_idx += 1;
                }
                DiskKind::HDD => {
                    label = format!("storage_hdd[{}]", hdd_idx);
                    hdd_idx += 1;
                }
                _ => continue,
            }
            // special label for AOOSTAR-X system panel
            add_sensor(
                sensors,
                format!("{label}_usage_percent"),
                (disk.total_space() - disk.available_space()) * 100 / disk.total_space(),
            );

            // using similar labels as AOOSTAR-X, but combining `{label2}_{label}`
            let device = disk.name().to_string_lossy().replace(' ', "_");
            add_sensor(
                sensors,
                format!("disk_{device}_total_bytes"),
                disk.total_space(),
            );
            add_sensor(
                sensors,
                format!("disk_{device}_total"),
                format_bytes(disk.total_space()),
            );
            let used = disk.total_space() - disk.available_space();
            add_sensor(sensors, format!("disk_{device}_used_bytes"), used);
            add_sensor(sensors, format!("disk_{device}_used"), format_bytes(used));
            add_sensor(
                sensors,
                format!("disk_{device}_free_bytes"),
                disk.available_space(),
            );
            add_sensor(
                sensors,
                format!("disk_{device}_free"),
                format_bytes(disk.available_space()),
            );
            add_sensor(
                sensors,
                format!("disk_{device}_usage_percent"),
                format!(
                    "{:.1}",
                    (disk.total_space() - disk.available_space()) as f64 * 100.0
                        / disk.total_space() as f64
                ),
            );
        }

        // Components temperature:
        for component in &self.components {
            if let Some(temperature) = component.temperature() {
                let label;
                if component.label().contains("spd5118") {
                    label = "temperature_memory".to_string();
                } else if component.label().contains("amdgpu") {
                    label = "temperature_gpu".to_string();
                } else if component.label().contains("Tctl") {
                    label = "temperature_cpu".to_string();
                } else if component.label().contains("Composite")
                    && !component.label().contains("nvme")
                {
                    // just a guess...
                    label = "temperature_motherboard".to_string();
                } else {
                    label = format!("temperature_{}", component.label().replace(' ', "_"));
                    // println!("label={}, type_id={:?}, id={:?}, {component:?}",
                    //          component.label(), component.type_id(), component.id());
                }

                add_sensor(sensors, format!("{label}#unit"), "°C");
                add_sensor(sensors, label, format!("{temperature:.1}"));
            }
        }

        // Network interfaces name, total data received and total data transmitted:
        for (interface_name, data) in &self.networks {
            // only consider specific interfaces
            let if_name = interface_name.to_lowercase();
            if !["eth", "en", "em", "wlan", "wlp", "wlo"]
                .iter()
                .any(|i| if_name.starts_with(*i))
            {
                continue;
            }
            // Sort by address to avoid random order in refreshes
            for (idx, addr) in data
                .ip_networks()
                .iter()
                .map(|net| net.addr)
                .sorted()
                .enumerate()
            {
                add_sensor(
                    sensors,
                    format!("network_{interface_name}_address{idx}"),
                    addr,
                );
            }

            if let Some(refresh) = self.refresh_duration {
                let interval = refresh.as_millis() as u64;
                if interval > 0 {
                    add_sensor(
                        sensors,
                        format!("network_{interface_name}_download_speed"),
                        format!("{}/s", format_bytes(1000 * data.received() / interval)),
                    );
                    add_sensor(
                        sensors,
                        format!("network_{interface_name}_upload_speed"),
                        format!("{}/s", format_bytes(1000 * data.transmitted() / interval)),
                    );
                }
            }

            add_sensor(
                sensors,
                format!("network_{interface_name}_total_received_bytes"),
                data.total_received(),
            );
            add_sensor(
                sensors,
                format!("network_{interface_name}_total_received"),
                format_bytes(data.total_received()),
            );
            add_sensor(
                sensors,
                format!("network_{interface_name}_total_transmitted_bytes"),
                data.total_transmitted(),
            );
            add_sensor(
                sensors,
                format!("network_{interface_name}_total_transmitted"),
                format_bytes(data.total_transmitted()),
            );
        }

        Ok(())
    }
}

fn add_sensor(
    sensors: &mut HashMap<String, String>,
    label: impl Into<String>,
    value: impl Display,
) {
    sensors.insert(label.into(), value.to_string());
}

fn update_linux_storage_sensors(
    sensors: &mut HashMap<String, String>,
    use_smartctl: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Note: AOOSTAR-X only considered spinning Rust. Too bad if you're using SSDs in the HD bays...
    if let Ok(hdd_devices) = get_storage_devices(StorageDevice::HddOrSsd) {
        debug!("HDD devices : {:?}", hdd_devices);
        for (idx, device) in hdd_devices.iter().enumerate() {
            let usage = get_disk_usage(device)?;
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]_total_size_bytes"),
                usage.total_size,
            );
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]_total_size"),
                format_bytes(usage.total_size),
            );
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]_total_used_bytes"),
                usage.total_used,
            );
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]_total_used"),
                format_bytes(usage.total_used),
            );
            add_sensor(
                sensors,
                format!("storage_hdd[{idx}]_usage_percent"),
                usage.usage_percent,
            );

            if use_smartctl && let Some(temperature) = get_smartctl_disk_temperature(device)? {
                add_sensor(
                    sensors,
                    format!("storage_hdd[{idx}]_temperature"),
                    temperature,
                );
            }
        }
    }

    // AOOSTAR-X: ssd == nvme
    if let Ok(nvme_devices) = get_storage_devices(StorageDevice::Nvme) {
        debug!("NVME devices: {:?}", nvme_devices);
        for (idx, device) in nvme_devices.iter().enumerate() {
            let usage = get_disk_usage(device)?;
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]_total_size_bytes"),
                usage.total_size,
            );
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]_total_size"),
                format_bytes(usage.total_size),
            );
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]_total_used_bytes"),
                usage.total_used,
            );
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]_total_used"),
                format_bytes(usage.total_used),
            );
            add_sensor(
                sensors,
                format!("storage_ssd[{idx}]_usage_percent"),
                usage.usage_percent,
            );

            if use_smartctl && let Some(temperature) = get_smartctl_disk_temperature(device)? {
                add_sensor(
                    sensors,
                    format!("storage_ssd[{idx}]_temperature"),
                    temperature,
                );
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
pub struct DiskInfo {
    pub device: String,
    pub temperature: i32,
    pub used: f64,
    pub total_used: u64,
    pub total_size: u64,
}

#[derive(Debug)]
pub struct DiskUsage {
    pub usage_percent: f64,
    pub total_used: u64,
    pub total_size: u64,
}

#[derive(Debug, PartialEq)]
pub enum StorageDevice {
    All,
    Hdd,
    Ssd,
    HddOrSsd,
    Nvme,
}

pub type DiskResult = Result<Vec<DiskInfo>, Box<dyn std::error::Error>>;

/// Get storage devices of the given type: NVME, SSD or HD
///
/// Storage devices are identified from /sys/block attributes.
/// Removable devices are excluded.
///
/// # Arguments
///
/// * `kind`: type of storage device
///
/// returns: sorted list of found device names (`sd*` and `nvme*`)
pub fn get_storage_devices(kind: StorageDevice) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut devices = Vec::new();
    let sys_block = Path::new("/sys/block");

    if !sys_block.exists() {
        info!("No storage device found");
        return Ok(devices);
    }

    let device_regex = Regex::new(r"^sd[a-z]+$")?;
    let nvme_regex = Regex::new(r"^nvme[0-9]+n[0-9]+$")?;

    for entry in fs::read_dir(sys_block)? {
        let entry = entry?;
        let dev_name = entry.file_name();
        let dev_str = dev_name.to_string_lossy();

        // filter out all non sd* and nvme* devices
        let is_nvme = nvme_regex.is_match(&dev_str);
        let is_storage = device_regex.is_match(&dev_str);
        if !(is_nvme || is_storage) {
            continue;
        }

        match kind {
            StorageDevice::All => {}
            StorageDevice::Hdd | StorageDevice::Ssd | StorageDevice::HddOrSsd => {
                if !is_storage {
                    continue;
                }
            }
            StorageDevice::Nvme => {
                if !is_nvme {
                    continue;
                }
            }
        };

        if is_nvme {
            let dev_name = entry.file_name();
            let dev_str = dev_name.to_string_lossy();
            devices.push(dev_str.to_string());
            continue;
        }

        let rotational_path = sys_block.join(dev_str.as_ref()).join("queue/rotational");
        let removable_path = sys_block.join(dev_str.as_ref()).join("removable");

        match (
            fs::read_to_string(&rotational_path),
            fs::read_to_string(&removable_path),
        ) {
            (Ok(rotational), Ok(removable)) => {
                let rotational = rotational.trim();
                let removable = removable.trim();

                // ignore removable
                if removable == "1" {
                    continue;
                }

                if kind == StorageDevice::Hdd && rotational == "1"
                    || kind == StorageDevice::Ssd && rotational == "0"
                    || kind == StorageDevice::HddOrSsd
                {
                    devices.push(dev_str.to_string());
                }
            }
            (Err(e), _) | (_, Err(e)) => {
                error!("Unable to read device {dev_str} attributes: {e}");
            }
        }
    }

    devices.sort();
    Ok(devices)
}

/// Retrieve temperature from NVMe or SDD/HDD with smartctl
pub fn get_smartctl_disk_temperature(dev: &str) -> Result<Option<i32>, Box<dyn std::error::Error>> {
    let temp_regex =
        Regex::new(r"194\s+Temperature_Celsius\s+\S+\s+\S+\s+\S+\s+\S+\s+\S+\s+\S+\s+-\s+(\d+)")?;
    let nvme_temp_regex = Regex::new(r"Temperature:\s+(\d+)\s")?;

    let dev = format!("/dev/{}", dev);
    match Command::new("sudo")
        .arg("-n")
        .arg("smartctl")
        .arg("-A")
        .arg(&dev)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);

            if let Some(temp_captures) = temp_regex
                .captures(&stdout)
                .or_else(|| nvme_temp_regex.captures(&stdout))
                && let Some(temp_match) = temp_captures.get(1)
            {
                let temperature = temp_match.as_str().parse::<i32>()?;
                return Ok(Some(temperature));
            }
        }
        Err(e) => {
            error!("Device {dev} acquisition failed, error: {e}");
        }
    }

    Ok(None)
}

/// Calculate actual filesystem usage rate of hard disk (based on df command)
pub fn get_disk_usage(dev: &str) -> Result<DiskUsage, Box<dyn std::error::Error>> {
    let mut tmp = DiskUsage {
        usage_percent: 0.0,
        total_used: 0,
        total_size: 0,
    };

    // Get mounted partitions for this device
    let cmd = format!(
        "df -h --output=source,target,pcent | grep '/dev/{}[0-9]*'",
        dev
    );

    match Command::new("sh").arg("-c").arg(&cmd).output() {
        Ok(output) => {
            if !output.status.success() {
                return Ok(tmp);
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut total_used: u64 = 0;
            let mut total_size: u64 = 0;

            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 3 {
                    continue;
                }

                let mountpoint = parts[1];

                // Get size in bytes
                let size_cmd = format!(
                    "df --block-size=1 {} | awk 'NR==2 {{print $2}}'",
                    mountpoint
                );
                if let Ok(size_output) = Command::new("sh").arg("-c").arg(&size_cmd).output()
                    && let Ok(size_str) = String::from_utf8(size_output.stdout)
                    && let Ok(size) = size_str.trim().parse::<u64>()
                {
                    total_size += size;
                }

                // Get used space in bytes
                let used_cmd = format!(
                    "df --block-size=1 {} | awk 'NR==2 {{print $3}}'",
                    mountpoint
                );
                if let Ok(used_output) = Command::new("sh").arg("-c").arg(&used_cmd).output()
                    && let Ok(used_str) = String::from_utf8(used_output.stdout)
                    && let Ok(used) = used_str.trim().parse::<u64>()
                {
                    total_used += used;
                }
            }

            if total_size != 0 {
                tmp.usage_percent =
                    ((total_used as f64 / total_size as f64) * 100.0 * 100.0).round() / 100.0;
                tmp.total_used = total_used;
                tmp.total_size = total_size;
            }

            Ok(tmp)
        }
        Err(_) => Ok(tmp),
    }
}

/// Format bytes into human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    const THRESHOLD: f64 = 1024.0;

    if bytes == 0 {
        return "0 B".to_string();
    }

    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= THRESHOLD && unit_index < UNITS.len() - 1 {
        size /= THRESHOLD;
        unit_index += 1;
    }

    if unit_index > 0 {
        format!("{:.2} {}", size, UNITS[unit_index])
    } else {
        format!("{} {}", size, UNITS[unit_index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }
}
