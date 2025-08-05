// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

//! AOOSTAR-X json configuration file format.
//!
//! Derived from the available Monitor3.json file in AOOSTAR-X v1.3.4.
//! Likely not fully compatible with files created with the original editor.

use image::Rgb;
use imageproc::definitions::HasWhite;
use log::warn;
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::io::BufReader;
use std::num::ParseIntError;
use std::ops::Deref;
use std::path::Path;
use std::{fmt, fs};

pub fn load_cfg<P: AsRef<Path>>(path: P) -> anyhow::Result<MonitorConfig> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let config: MonitorConfig = serde_json::from_reader(reader)?;

    for active in config.active_panels.clone() {
        if active == 0 || active > config.panels.len() as u32 {
            warn!("Ignoring invalid active panel {active}");
            continue;
        }
        let panel = &config.panels[active as usize - 1];

        println!(
            "Panel {active}: type={}, {}",
            panel.panel_type,
            panel.img.as_deref().unwrap_or_default()
        );
        for sensor in &panel.sensor {
            println!(
                "  {}: {} {} {}",
                sensor.label,
                sensor
                    .name
                    .as_deref()
                    .or(sensor.item_name.as_deref())
                    .unwrap_or_default(),
                sensor.value.as_deref().unwrap_or_default(),
                sensor.unit.as_deref().unwrap_or_default()
            );
        }
    }

    Ok(config)
}

/// AOOSTAR-X monitor json configuration file
#[derive(Debug, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// _Not used_
    pub credentials: Option<Credentials>,
    pub setup: Setup,
    /// Panels: 1-based index into diy[i]
    #[serde(rename = "mianban")]
    pub active_panels: Vec<u32>,
    /// Custom panels / DIY "Do It Yourself",
    #[serde(rename = "diy")]
    pub panels: Vec<Panel>,
    /// Internal index of the currently active panel. 1-based!
    #[serde(skip)]
    active_panel_idx: Option<usize>,
}

impl MonitorConfig {
    pub fn get_next_active_panel(&mut self) -> Option<&Panel> {
        let mut active_panel_idx = self.active_panel_idx.unwrap_or(0) + 1;
        if active_panel_idx > self.panels.len() {
            active_panel_idx = 1;
        }

        for (index, active) in self
            .active_panels
            .iter()
            .filter(|&active| *active > 0)
            .enumerate()
        {
            if *active > self.panels.len() as u32 {
                warn!("Ignoring invalid active panel {active}");
                continue;
            }
            if index + 1 == active_panel_idx {
                self.active_panel_idx = Some(active_panel_idx);
                return Some(&self.panels[*active as usize - 1]);
            }
        }

        None
    }
}

/// Web-app user login
#[derive(Debug, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// Configuration Settings
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    /// Default: true
    pub off_display: bool,
    /// Selection of default panels based on theme / control_params / control_disk_temp ?
    pub theme: i32,
    /// ? Default: true
    pub control_params: bool,
    /// ? Default: true
    pub control_disk_temp: bool,
    /// Default: false
    pub custom_panel: bool,
    /// Language index. Default: 0
    pub language: Language,
    /// Switch time between panels (?) in seconds, interpreted as int. Default: 5
    pub switch_time: Option<String>, // existed as "30" string
    /// Operation mode: performance, power saving, etc.
    pub operation_mode: Option<OperationMode>,
    /// Operation type 1 or 2 (?). Default: 1
    #[serde(rename = "type")]
    pub operation_type: Option<i16>,
    /// Default: 300
    pub disk_update: i32,
    /// Home Assistant URL
    #[serde(deserialize_with = "empty_string_as_none")]
    #[serde(rename = "ha_url")]
    pub ha_url: Option<String>, // "" in JSON ⇒ Option<String>
    /// Home Assistant long-lived access token
    #[serde(deserialize_with = "empty_string_as_none")]
    #[serde(rename = "ha_token")]
    pub ha_token: Option<String>, // "" in JSON ⇒ Option<String>
    /// Panel refresh in seconds. Default: 1
    pub refresh: f32,
}

#[derive(Debug, Serialize_repr, Deserialize_repr, PartialEq)]
#[repr(u8)]
pub enum Language {
    Chinese = 0,
    English = 1,
    Japanese = 2,
}

#[derive(Debug, Serialize_repr, Deserialize_repr, PartialEq)]
#[repr(i16)]
pub enum OperationMode {
    None = -1,
    HighPerformance = 0,
    Intelligent = 1,
    PowerSaving = 2,
    Custom30W = 3,
    Custom20W = 4,
    Custom10W = 5,
}

/// Custom DIY panel definition
#[derive(Debug, Serialize, Deserialize)]
pub struct Panel {
    /// Custom panel id
    pub id: Option<String>,
    /// Custom panel name
    pub name: Option<String>,
    /// TODO
    pub checked: Option<bool>,
    /// TODO panel type: 5 = built-in? 6 = custom ?
    #[serde(rename = "type")]
    pub panel_type: i32,
    /// Background image filename
    pub img: Option<String>,
    /// Sensors
    pub sensor: Vec<Sensor>,
}

/// One Data Display Unit
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sensor {
    /// Sensor mode: text, fan, progress, pointer
    pub mode: SensorMode,
    /// Sensor type. TODO verify sensor type values
    /// - 1 Time / Date Labels
    /// - 2 Windows-specific system info
    /// - 3 Hardware value
    /// - 4 AIDA64 sensor
    /// - 5 HA sensor
    /// - 6 http fetch from url
    /// - 7 system info ?
    /// - 8 lm-sensor ?
    #[serde(rename = "type")]
    pub sensor_type: i32,
    /// Label name for internal panels.
    pub name: Option<String>,
    /// Label name for custom panels.
    pub item_name: Option<String>,
    /// TODO Data source?
    pub label: String,

    /// x-position. Custom panel coordinates are stored as float!
    pub x: f32,
    /// x-position. TODO unit
    pub y: f32,
    pub width: Option<i32>,
    pub height: Option<i32>,

    pub text_direction: i32, // layout direction
    pub direction: i32,      // sensor orientation, 0/1

    #[serde(deserialize_with = "empty_string_as_none")]
    pub value: Option<String>, // "" or numbers, so Option<String>

    pub font_family: String,
    pub font_size: i32,
    /// Font color in `#RRGGBB` notation, or -1 if not set. #ffffff = white, #ff0000 = red
    pub font_color: FontColor,
    pub font_weight: FontWeight,
    pub text_align: TextAlign,

    #[serde(deserialize_with = "option_none_if_minus_one")]
    pub integer_digits: Option<i32>, // -1 ≈ unset ⇒ Option<i32>
    #[serde(deserialize_with = "option_none_if_minus_one")]
    pub decimal_digits: Option<i32>, // -1 ≈ unset ⇒ Option<i32>

    /// Optional unit text to print after the value
    #[serde(deserialize_with = "empty_string_as_none")]
    pub unit: Option<String>,

    pub min_angle: i32,
    pub max_angle: i32,
    pub min_value: i32,
    pub max_value: i32,

    /// TODO determine meaning of: pic - render picture?
    #[serde(deserialize_with = "empty_string_as_none")]
    pub pic: Option<String>, // "" when unused
    /// Pivot x
    #[serde(rename = "xz_x")]
    pub xz_x: Option<i32>,
    /// Pivot y
    #[serde(rename = "xz_y")]
    pub xz_y: Option<i32>,

    /// For type = 6
    pub url: Option<String>,
    /// For type = 6
    pub data: Option<String>,
    /// For type = 6
    pub interval: Option<u32>,
}

#[derive(Debug, Serialize_repr, Deserialize_repr, PartialEq)]
#[repr(u8)]
pub enum SensorMode {
    Text = 1,
    Fan = 2,
    Progress = 3,
    Pointer = 4,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TimeDateLabel {
    #[serde(rename = "DATE_year")]
    Year,
    #[serde(rename = "DATE_month")]
    Month,
    #[serde(rename = "DATE_day")]
    Day,
    #[serde(rename = "DATE_hour")]
    Hour,
    #[serde(rename = "DATE_minute")]
    Minute,
    #[serde(rename = "DATE_second")]
    Second,
    #[serde(rename = "DATE_m_d_h_m_1")]
    MDHM1,
    #[serde(rename = "DATE_m_d_h_m_2")]
    MDHM2,
    #[serde(rename = "DATE_m_d_1")]
    MD1,
    #[serde(rename = "DATE_m_d_2")]
    MD2,
    #[serde(rename = "DATE_y_m_d_1")]
    YMD1,
    #[serde(rename = "DATE_y_m_d_2")]
    YMD2,
    #[serde(rename = "DATE_y_m_d_3")]
    YMD3,
    #[serde(rename = "DATE_y_m_d_4")]
    YMD4,
    #[serde(rename = "DATE_h_m_s_1")]
    HMS1,
    #[serde(rename = "DATE_h_m_s_2")]
    HMS2,
    #[serde(rename = "DATE_h_m_s_3")]
    HMS3,
    #[serde(rename = "DATE_h_m_1")]
    HM1,
    #[serde(rename = "DATE_h_m_2")]
    HM2,
    #[serde(rename = "DATE_h_m_3")]
    HM3,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontWeight {
    Normal,
    Bold,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

fn option_none_if_minus_one<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<i32>::deserialize(deserializer)? {
        Some(-1) | None => Ok(None),
        Some(other) => Ok(Some(other)),
    }
}

/// Special font color type since it is represented either as numeric -1 or as a string :-(
///
/// A good serde programming exercise...
#[derive(Debug, Clone, Copy)]
pub struct FontColor(Rgb<u8>);

impl Default for FontColor {
    fn default() -> Self {
        FontColor(Rgb::white())
    }
}

impl Deref for FontColor {
    type Target = Rgb<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<&str> for FontColor {
    type Error = ParseIntError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.len() != 7 || value.starts_with('#') {
            warn!("Invalid font color: {value}");
            Ok(FontColor::default())
        } else {
            Ok(FontColor(Rgb([
                u8::from_str_radix(&value[1..3], 16)?,
                u8::from_str_radix(&value[3..5], 16)?,
                u8::from_str_radix(&value[5..7], 16)?,
            ])))
        }
    }
}

impl From<Rgb<u8>> for FontColor {
    fn from(value: Rgb<u8>) -> Self {
        FontColor(value)
    }
}

impl From<FontColor> for Rgb<u8> {
    fn from(val: FontColor) -> Self {
        val.0
    }
}

impl Serialize for FontColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        format!("#{:02x}{:02x}{:02x}", self.0[0], self.0[1], self.0[2]).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FontColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyVisitor;

        impl<'de> Visitor<'de> for MyVisitor {
            type Value = FontColor;

            fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt.write_str("integer or string")
            }

            fn visit_i64<E>(self, val: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match val {
                    -1 => Ok(FontColor::default()),
                    _ => Err(E::custom("invalid integer value, expected -1")),
                }
            }

            fn visit_str<E>(self, val: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if val.trim().is_empty() {
                    return Ok(FontColor::default());
                }
                match val.parse::<i32>() {
                    Ok(val) => self.visit_i32(val),
                    Err(_) => val
                        .try_into()
                        .map_err(|e| E::custom(format!("invalid font color value: {e}"))),
                }
            }
        }

        deserializer.deserialize_any(MyVisitor)
    }
}

fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let option = Option::<String>::deserialize(deserializer)?;
    Ok(option.and_then(|s| if s.trim().is_empty() { None } else { Some(s) }))
}
