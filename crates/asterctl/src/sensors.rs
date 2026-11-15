// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

//! Sensor value sources.
//!
//! Implementations:
//! - internal date time sensors
//! - file-based value provider with simple key-value pairs, with stale value detection.

use chrono::{DateTime, Datelike, Local, Timelike};
use log::{debug, error, info, warn};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::{Arc, RwLock, mpsc};
use std::time::{Duration, SystemTime};

pub fn get_date_time_value(label: &str, now: &DateTime<Local>) -> Option<String> {
    if !label.starts_with("DATE_") {
        return None;
    }

    let year = now.year();
    let month = format!("{:02}", now.month());
    let day = format!("{:02}", now.day());
    let hour = format!("{:02}", now.hour());
    let minute = format!("{:02}", now.minute());
    let second = format!("{:02}", now.second());

    // same formatting logic as in AOOSTAR-X
    let value = match label {
        "DATE_year" => year.to_string(),
        "DATE_month" => month,
        "DATE_day" => day,
        "DATE_hour" => hour,
        "DATE_minute" => minute,
        "DATE_second" => second,
        "DATE_m_d_h_m_1" => format!("{month}月{day}日  {hour}:{minute}"),
        "DATE_m_d_h_m_2" => format!("{month}/{day}  {hour}:{minute}"),
        "DATE_m_d_1" => format!("{month}月{day}日"),
        "DATE_m_d_2" => format!("{month}-{day}"),
        "DATE_y_m_d_1" => format!("{year}年{month}月{day}日"),
        "DATE_y_m_d_2" => format!("{year}-{month}-{day}"),
        "DATE_y_m_d_3" => format!("{year}/{month}/{day}"),
        "DATE_y_m_d_4" => format!("{year} {month} {day}"),
        "DATE_h_m_s_1" => format!("{hour}:{minute}:{second}"),
        "DATE_h_m_s_2" => format!("{hour}时{minute}分{second}秒"),
        "DATE_h_m_s_3" => format!("{hour} {minute} {second}"),
        "DATE_h_m_1" => format!("{hour}时{minute}分"),
        "DATE_h_m_2" => format!("{hour} : {minute}"),
        "DATE_h_m_3" => format!("{hour}:{minute}"),
        _ => return None,
    };

    Some(value)
}

/// Placeholder value rendered for sensors whose source file is no longer being refreshed.
pub const STALE_VALUE: &str = "--";

/// Sensor values collected from one or more sensor source files.
///
/// Every value remembers the source file it was read from. When a source file is updated, keys
/// that are no longer present in the file are removed, and when a source file stops being updated
/// its values can be reported as stale with [SensorStore::snapshot].
#[derive(Debug, Default)]
pub struct SensorStore {
    values: HashMap<String, String>,
    owner: HashMap<String, PathBuf>,
    sources: HashMap<PathBuf, SourceFile>,
}

#[derive(Debug)]
struct SourceFile {
    keys: HashSet<String>,
    modified: SystemTime,
}

impl SensorStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace all values of the given source file.
    ///
    /// Keys from a previous version of the file that are no longer present are removed, unless
    /// another source file has provided the same key since.
    pub fn update_source(
        &mut self,
        path: &Path,
        new_values: HashMap<String, String>,
        modified: SystemTime,
    ) {
        let new_keys: HashSet<String> = new_values.keys().cloned().collect();
        if let Some(old) = self.sources.get(path) {
            for key in old.keys.difference(&new_keys) {
                if self.owner.get(key).map(PathBuf::as_path) == Some(path) {
                    self.values.remove(key);
                    self.owner.remove(key);
                }
            }
        }
        for (key, value) in new_values {
            self.owner.insert(key.clone(), path.to_path_buf());
            self.values.insert(key, value);
        }
        self.sources.insert(
            path.to_path_buf(),
            SourceFile {
                keys: new_keys,
                modified,
            },
        );
    }

    /// Remove all values of a deleted source file.
    pub fn remove_source(&mut self, path: &Path) {
        self.update_source(path, HashMap::new(), SystemTime::now());
        self.sources.remove(path);
    }

    /// Last modification time of a known source file.
    pub fn source_modified(&self, path: &Path) -> Option<SystemTime> {
        self.sources.get(path).map(|s| s.modified)
    }

    /// Current values, without staleness handling.
    pub fn values(&self) -> &HashMap<String, String> {
        &self.values
    }

    /// Copy of all values for rendering.
    ///
    /// Values of source files that have not been modified within `stale_after` are replaced with
    /// [STALE_VALUE], and their `#unit` entries are dropped. A panel showing old values as if they
    /// were live is worse than one admitting that it is blind.
    pub fn snapshot(
        &self,
        stale_after: Option<Duration>,
        now: SystemTime,
    ) -> HashMap<String, String> {
        let Some(stale_after) = stale_after else {
            return self.values.clone();
        };

        let mut snapshot = HashMap::with_capacity(self.values.len());
        for (key, value) in &self.values {
            let stale = self
                .owner
                .get(key)
                .and_then(|path| self.sources.get(path))
                .map(|source| now.duration_since(source.modified).unwrap_or_default() > stale_after)
                .unwrap_or_default();
            if !stale {
                snapshot.insert(key.clone(), value.clone());
            } else if !key.ends_with("#unit") {
                snapshot.insert(key.clone(), STALE_VALUE.to_string());
            }
        }
        snapshot
    }
}

/// Fallback interval for re-reading changed sensor files, in case file events are missed, or a
/// watched directory didn't exist yet or has been re-created.
const RESCAN_INTERVAL: Duration = Duration::from_secs(5);

/// Read all sensor value source files from the given paths and start monitoring for changes.
///
/// Each source path is either a single sensor source file or a directory containing multiple
/// sensor source files with a `.txt` extension.
///
/// The source paths are monitored for changes in a separate thread, and additionally re-scanned
/// every few seconds. Updated files are automatically read and stored in the shared store.
/// Source paths that don't exist yet are picked up as soon as they appear.
///
/// # Arguments
///
/// * `source_paths`: Source file paths and / or directory paths.
/// * `store`: a shared, reader-writer lock protected sensor store
/// * `sensor_filter`: Optional list of regex filters to filter out matching sensor keys.
///
/// returns: Result<(), Error>
pub fn start_file_slurper(
    source_paths: Vec<PathBuf>,
    store: Arc<RwLock<SensorStore>>,
    sensor_filter: Option<Vec<Regex>>,
) -> anyhow::Result<()> {
    // read existing file(s)
    {
        let mut store = store.write().expect("Failed to lock values");
        for path in &source_paths {
            scan_path(path, &mut store, sensor_filter.as_deref());
        }
    }

    std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to initialize watcher: {e}");
                exit(1);
            }
        };

        // Single files are watched through their parent directory: atomic updates replace the
        // file, and a watch on the file itself would be lost after the first rename.
        let watch_dirs: Vec<PathBuf> = source_paths
            .iter()
            .map(|path| {
                if path.is_dir() {
                    path.clone()
                } else {
                    path.parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| PathBuf::from("."))
                }
            })
            .collect();
        let mut watched = vec![false; watch_dirs.len()];

        info!("Starting sensor file watcher for {source_paths:?} with filter {sensor_filter:?}");
        loop {
            for (idx, dir) in watch_dirs.iter().enumerate() {
                if !watched[idx] && dir.is_dir() {
                    match watcher.watch(dir, RecursiveMode::NonRecursive) {
                        Ok(_) => {
                            debug!("Watching sensor directory {dir:?}");
                            watched[idx] = true;
                        }
                        Err(e) => warn!("Failed to watch sensor directory {dir:?}: {e}"),
                    }
                } else if watched[idx] && !dir.is_dir() {
                    // directory removed: watch is gone, re-add once it's back
                    let _ = watcher.unwatch(dir);
                    watched[idx] = false;
                }
            }

            match rx.recv_timeout(RESCAN_INTERVAL) {
                Ok(Ok(event)) => {
                    if matches!(event.kind, EventKind::Access(_)) {
                        continue;
                    }
                    for path in event.paths.iter() {
                        if !is_sensor_file_of(path, &source_paths) {
                            continue;
                        }
                        debug!("Sensor file event {:?}: {path:?}", event.kind);
                        let mut store = store.write().expect("Poisoned sensor RwLock");
                        reload_file(path, &mut store, sensor_filter.as_deref());
                    }
                }
                Ok(Err(e)) => warn!("watch error: {e:?}"),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let mut store = store.write().expect("Poisoned sensor RwLock");
                    for path in &source_paths {
                        scan_path(path, &mut store, sensor_filter.as_deref());
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    error!("Sensor file watcher stopped");
                    exit(1);
                }
            }
        }
    });

    Ok(())
}

/// Check if the path is a sensor file belonging to one of the configured source paths.
fn is_sensor_file_of(path: &Path, source_paths: &[PathBuf]) -> bool {
    source_paths.iter().any(|source| {
        path == source
            || (source.is_dir()
                && path.parent() == Some(source.as_path())
                && path.extension().unwrap_or_default() == "txt")
    })
}

/// (Re-)read a sensor file, or remove its values if it no longer exists.
fn reload_file(path: &Path, store: &mut SensorStore, sensor_filter: Option<&[Regex]>) {
    let modified = match fs::metadata(path) {
        Ok(meta) if meta.is_file() => meta.modified().unwrap_or_else(|_| SystemTime::now()),
        _ => {
            if store.source_modified(path).is_some() {
                info!("Sensor file removed: {path:?}");
                store.remove_source(path);
            }
            return;
        }
    };

    let mut values = HashMap::new();
    match read_key_value_file(path, &mut values, sensor_filter) {
        Ok(_) => store.update_source(path, values, modified),
        Err(e) => warn!("Failed to read sensor file {path:?}: {e}"),
    }
}

/// Read all changed sensor files of a source path: a single file or all `.txt` files in a
/// directory. Removed files are dropped from the store.
fn scan_path(path: &Path, store: &mut SensorStore, sensor_filter: Option<&[Regex]>) {
    let mut files = Vec::new();
    if path.is_dir() {
        match fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let file = entry.path();
                    if file.is_file() && file.extension().unwrap_or_default() == "txt" {
                        files.push(file);
                    }
                }
            }
            Err(e) => warn!("Failed to read sensor directory {path:?}: {e}"),
        }
    } else if path.is_file() {
        files.push(path.to_path_buf());
    }

    // removed files
    let known: Vec<PathBuf> = store
        .sources
        .keys()
        .filter(|known| {
            (known.as_path() == path || known.parent() == Some(path)) && !files.contains(known)
        })
        .cloned()
        .collect();
    for file in known {
        reload_file(&file, store, sensor_filter);
    }

    for file in files {
        let modified = fs::metadata(&file).and_then(|m| m.modified()).ok();
        if modified.is_none() || modified != store.source_modified(&file) {
            reload_file(&file, store, sensor_filter);
        }
    }
}

/// Read a key-value-based sensor source file and store content in the provided hashmap.
///
/// - Empty lines are skipped
/// - Lines starting with # are skipped
/// - Key-value pairs must be separated by `:`
/// - All keys and values are trimmed
///
/// # Arguments
///
/// * `path`: file path to read.
/// * `values`: HashMap to insert key-value pairs from the file.
/// * `sensor_filter`: Optional list of regex filters to filter out matching sensor keys.
///
/// returns: Result<(), Error>
pub fn read_key_value_file<P: AsRef<Path>>(
    path: P,
    values: &mut HashMap<String, String>,
    sensor_filter: Option<&[Regex]>,
) -> anyhow::Result<()> {
    debug!("Reading sensor file {:?}", path.as_ref());

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            if let Some(filter) = sensor_filter
                && is_filtered(key, filter)
            {
                debug!("Filtered: {key}");
                continue;
            }

            values.insert(key.trim().to_string(), value.trim().to_string());
        } else {
            warn!("Skipping invalid entry in sensor value file: {line}");
        }
    }

    Ok(())
}

fn is_filtered(key: &str, filters: &[Regex]) -> bool {
    filters.iter().any(|re| re.is_match(key))
}

/// Read the sensor filter configuration file.
///
/// This is a simple text file containing multiple RegEx expressions.
/// - one RegEx per line
/// - Empty lines are skipped
/// - Lines starting with # are skipped
///
/// # Arguments
///
/// * `path`: file path to read.
///
/// returns: None if the file is empty or contains no valid RegEx expressions.
///
pub fn read_filter_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Option<Vec<Regex>>> {
    debug!("Reading sensor filter file {:?}", path.as_ref());

    let mut filters = Vec::new();
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match Regex::new(line) {
            Ok(re) => {
                filters.push(re);
            }
            Err(e) => {
                warn!("Skipping invalid filter in sensor filter file: {line}: {e}");
            }
        }
    }

    if filters.is_empty() {
        Ok(None)
    } else {
        Ok(Some(filters))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn kv(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn update_source_removes_keys_missing_in_new_version() {
        let mut store = SensorStore::new();
        let file = Path::new("/run/a.txt");
        let now = SystemTime::now();
        store.update_source(
            file,
            kv(&[("vm[0]_name", "nas"), ("vm[1]_name", "docker")]),
            now,
        );
        store.update_source(file, kv(&[("vm[0]_name", "nas")]), now);

        assert_eq!(
            store.values().get("vm[0]_name").map(String::as_str),
            Some("nas")
        );
        assert!(!store.values().contains_key("vm[1]_name"));
    }

    #[test]
    fn update_source_keeps_keys_taken_over_by_other_source() {
        let mut store = SensorStore::new();
        let now = SystemTime::now();
        let a = Path::new("/run/a.txt");
        let b = Path::new("/run/b.txt");
        store.update_source(a, kv(&[("shared", "a")]), now);
        store.update_source(b, kv(&[("shared", "b")]), now);
        store.update_source(a, kv(&[]), now);

        assert_eq!(store.values().get("shared").map(String::as_str), Some("b"));
    }

    #[test]
    fn remove_source_drops_values() {
        let mut store = SensorStore::new();
        let file = Path::new("/run/a.txt");
        store.update_source(file, kv(&[("foo", "1")]), SystemTime::now());
        store.remove_source(file);

        assert!(store.values().is_empty());
        assert!(store.source_modified(file).is_none());
    }

    #[test]
    fn snapshot_marks_stale_sources() {
        let mut store = SensorStore::new();
        let now = SystemTime::now();
        let fresh = Path::new("/run/fresh.txt");
        let old = Path::new("/run/old.txt");
        store.update_source(fresh, kv(&[("cpu", "12")]), now);
        store.update_source(
            old,
            kv(&[("pool", "75"), ("pool#unit", "%")]),
            now - Duration::from_secs(120),
        );

        let snapshot = store.snapshot(Some(Duration::from_secs(30)), now);
        assert_eq!(snapshot.get("cpu").map(String::as_str), Some("12"));
        assert_eq!(snapshot.get("pool").map(String::as_str), Some(STALE_VALUE));
        assert!(!snapshot.contains_key("pool#unit"));

        let snapshot = store.snapshot(None, now);
        assert_eq!(snapshot.get("pool").map(String::as_str), Some("75"));
    }

    #[test]
    fn is_filtered_does_not_filter_without_filters() {
        let key = "foobar";
        let filters = Vec::new();
        assert!(!is_filtered(key, &filters));
    }

    #[test]
    fn test_unit_extension_filter() {
        let key = "temperature_cpu#unit";
        let filters = vec![Regex::new("^temperature_.*#unit").unwrap()];
        assert!(is_filtered(key, &filters));
    }

    #[rstest]
    #[case(vec!["^foo$"])]
    #[case(vec!["^bar"])]
    #[case(vec!["other"])]
    #[case(vec!["123", "bla", "other"])]
    fn is_filtered_does_not_filter_without_a_match(#[case] filters: Vec<&str>) {
        let key = "foobar";
        let filters: Vec<Regex> = filters
            .iter()
            .map(|f| Regex::new(f).expect("Invalid regex"))
            .collect();
        assert!(
            !is_filtered(key, &filters),
            "Filter {filters:?} should not match {key}"
        );
        //
    }

    #[rstest]
    #[case(vec!["foo"])]
    #[case(vec!["bar"])]
    #[case(vec!["^.+bar"])]
    #[case(vec!["123", "foo", "other"])]
    #[case(vec!["bar", "123"])]
    #[case(vec!["^.+bar", "other"])]
    fn is_filtered_matches_filters(#[case] filters: Vec<&str>) {
        let key = "foobar";
        let filters: Vec<Regex> = filters
            .iter()
            .map(|f| Regex::new(f).expect("Invalid regex"))
            .collect();
        assert!(
            is_filtered(key, &filters),
            "Filter {filters:?} match match {key}"
        );
    }
}
