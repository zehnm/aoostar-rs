// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sensor value collection and formatting helpers.

use std::collections::BTreeMap;
use std::fmt::Display;

/// Sensor values, sorted for readable output files.
#[derive(Debug, Default)]
pub struct Sensors(BTreeMap<String, String>);

/// Severity of a state value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Warn,
    Bad,
}

impl Level {
    pub fn suffix(&self) -> &'static str {
        match self {
            Level::Ok => "ok",
            Level::Warn => "warn",
            Level::Bad => "bad",
        }
    }
}

impl Sensors {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&mut self, key: impl Into<String>, value: impl Display) {
        let value = value.to_string().replace(['\n', '\r'], " ");
        self.0.insert(key.into(), value.trim().to_string());
    }

    /// Add a state value: `key` plus exactly one of `key_ok`, `key_warn` or `key_bad`.
    ///
    /// Sensor panels have one fixed color per text element. Placing three differently colored
    /// elements for the `_ok`, `_warn` and `_bad` variants at the same position renders the value
    /// in the color of its severity, since missing values are not drawn.
    pub fn put_state(&mut self, key: &str, value: impl Display, level: Level) {
        let value = value.to_string();
        self.put(format!("{key}_{}", level.suffix()), &value);
        self.put(key, value);
    }

    #[cfg(test)]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }
}

/// Collected problems for the summary panel.
#[derive(Debug, Default)]
pub struct Problems(Vec<(Level, String)>);

impl Problems {
    pub fn add(&mut self, level: Level, text: impl Into<String>) {
        self.0.push((level, text.into()));
    }

    pub fn count(&self) -> usize {
        self.0.len()
    }

    /// Write the summary: total count, severity variants and the first problem descriptions.
    pub fn write(&self, sensors: &mut Sensors, max_texts: usize) {
        let worst = self.0.iter().map(|(l, _)| *l).max().unwrap_or(Level::Ok);
        sensors.put_state("homelab_problems", self.count(), worst);

        let mut sorted: Vec<_> = self.0.iter().collect();
        sorted.sort_by(|a, b| b.0.cmp(&a.0));
        for (idx, (level, text)) in sorted.iter().take(max_texts).enumerate() {
            sensors.put_state(&format!("homelab_problem[{idx}]"), text, *level);
        }
        if self.0.is_empty() {
            sensors.put_state("homelab_problem[0]", "ALL SYSTEMS OK", Level::Ok);
        } else if sorted.len() > max_texts {
            sensors.put(
                "homelab_problems_more",
                format!("+{} MORE", sorted.len() - max_texts),
            );
        }
    }
}

/// Human-readable size with binary units, using the same unit labels as `aster-sysinfo`.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if size >= 100.0 {
        format!("{size:.0} {}", UNITS[unit])
    } else if size >= 10.0 {
        format!("{size:.1} {}", UNITS[unit])
    } else {
        format!("{size:.2} {}", UNITS[unit])
    }
}

/// Compact uptime: `3d 04h` for one day or more, otherwise `hh:mm`.
pub fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3600;
    let minutes = (seconds % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours:02}h")
    } else {
        format!("{hours:02}:{minutes:02}")
    }
}

/// Compact age: `45m`, `13h` (below 2 days) or `4d`.
pub fn format_age(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 2 * 86_400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86_400)
    }
}

/// Unix timestamp in seconds from an API value: `{"$date": ms}`, a number in seconds or
/// milliseconds, or an RFC 3339 string.
pub fn epoch_secs(value: &serde_json::Value) -> Option<i64> {
    use serde_json::Value;
    match value {
        Value::Object(map) => map
            .get("$date")
            .and_then(|d| d.as_f64())
            .map(|ms| (ms / 1000.0) as i64),
        Value::Number(n) => n.as_f64().map(|t| {
            if t > 1e11 {
                (t / 1000.0) as i64 // milliseconds
            } else {
                t as i64
            }
        }),
        Value::String(s) => chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.timestamp()),
        _ => None,
    }
}

pub fn percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 * 100.0 / total as f64
    }
}

/// Shorten text to a maximum number of characters, adding an ellipsis.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut short: String = text.chars().take(max.saturating_sub(1)).collect();
        short.push('…');
        short
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_writes_value_and_one_variant() {
        let mut s = Sensors::new();
        s.put_state("pool[0]_status", "DEGRADED", Level::Bad);
        assert_eq!(s.get("pool[0]_status"), Some("DEGRADED"));
        assert_eq!(s.get("pool[0]_status_bad"), Some("DEGRADED"));
        assert_eq!(s.get("pool[0]_status_ok"), None);
        assert_eq!(s.get("pool[0]_status_warn"), None);
    }

    #[test]
    fn put_removes_line_breaks() {
        let mut s = Sensors::new();
        s.put("alert", "first\nsecond ");
        assert_eq!(s.get("alert"), Some("first second"));
    }

    #[test]
    fn problems_summary() {
        let mut s = Sensors::new();
        Problems::default().write(&mut s, 3);
        assert_eq!(s.get("homelab_problems_ok"), Some("0"));
        assert_eq!(s.get("homelab_problem[0]"), Some("ALL SYSTEMS OK"));

        let mut p = Problems::default();
        p.add(Level::Warn, "1 WARNING");
        p.add(Level::Bad, "POOL tank DEGRADED");
        let mut s = Sensors::new();
        p.write(&mut s, 1);
        assert_eq!(s.get("homelab_problems_bad"), Some("2"));
        assert_eq!(s.get("homelab_problem[0]_bad"), Some("POOL tank DEGRADED"));
        assert_eq!(s.get("homelab_problems_more"), Some("+1 MORE"));
    }

    #[test]
    fn formatting() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(20 * 1024 * 1024 * 1024 * 1024), "20.0 TB");
        assert_eq!(format_uptime(3 * 86_400 + 4 * 3600 + 59), "3d 04h");
        assert_eq!(format_uptime(3 * 3600 + 7 * 60), "03:07");
        assert_eq!(truncate("homeassistant", 6), "homea…");
        assert_eq!(truncate("nas", 6), "nas");
        assert_eq!(format_age(59), "0m");
        assert_eq!(format_age(45 * 60), "45m");
        assert_eq!(format_age(13 * 3600), "13h");
        assert_eq!(format_age(47 * 3600), "47h");
        assert_eq!(format_age(4 * 86_400 + 100), "4d");
        assert_eq!(format_age(-5), "0m");
    }

    #[test]
    fn epoch_values() {
        use serde_json::json;
        assert_eq!(
            epoch_secs(&json!({"$date": 1_763_000_000_000i64})),
            Some(1_763_000_000)
        );
        assert_eq!(epoch_secs(&json!(1_763_000_000)), Some(1_763_000_000));
        assert_eq!(
            epoch_secs(&json!(1_763_000_000_123i64)),
            Some(1_763_000_000)
        );
        assert_eq!(
            epoch_secs(&json!("2026-11-15T10:00:00Z")),
            Some(1_794_736_800)
        );
        assert_eq!(epoch_secs(&json!(null)), None);
    }
}
