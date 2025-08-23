// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

//! AOOSTAR-X json configuration file format.
//!
//! Derived from the available Monitor3.json file in AOOSTAR-X v1.3.4.
//! Likely not fully compatible with files created with the original editor.

use anyhow::Context;
use image::{Rgb, Rgba};
use imageproc::definitions::HasWhite;
use log::warn;
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::io::BufReader;
use std::num::ParseIntError;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::{fmt, fs};

pub fn load_cfg<P: AsRef<Path>>(path: P) -> anyhow::Result<MonitorConfig> {
    let path = path.as_ref();
    let file = fs::File::open(path).with_context(|| format!("Failed to load config {path:?}"))?;
    let reader = BufReader::new(file);
    let config: MonitorConfig = serde_json::from_reader(reader)?;

    for active in config.active_panels.clone() {
        if active == 0 || active > config.panels.len() as u32 {
            warn!("Ignoring invalid active panel {active}");
            continue;
        }
        let panel = &config.panels[active as usize - 1];

        println!(
            "Panel {active}: {}",
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
    // _Not used_
    // pub credentials: Option<Credentials>,
    /// Configuration settings.
    pub setup: Setup,
    /// Panels: 1-based index into `panels`
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
///
/// Not used, part of AOOSTAR-X json configuration file.
#[derive(Debug, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// Configuration settings.
///
/// Note: Trimmed down object to include only required fields for `asterctl`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    /// Switch time between panels in seconds, interpreted as float and converted to milliseconds. Default: 5
    pub switch_time: Option<String>, // existed as "30" string
    /// Panel redraw interval in seconds. Default: 1
    pub refresh: f32,
    /*
    // The following fields of the AOOSTAR-X json configuration file are NOT used in `asterctl`
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
    */
}

/// Language setting.
///
/// Not used, part of AOOSTAR-X json configuration file.
#[derive(Debug, Copy, Clone, Serialize_repr, Deserialize_repr, PartialEq)]
#[repr(u8)]
#[allow(dead_code)]
pub enum Language {
    Chinese = 0,
    English = 1,
    Japanese = 2,
}

/// Not used, part of AOOSTAR-X json configuration file.
#[derive(Debug, Copy, Clone, Serialize_repr, Deserialize_repr, PartialEq)]
#[repr(i16)]
#[allow(dead_code)]
pub enum OperationMode {
    None = -1,
    HighPerformance = 0,
    Intelligent = 1,
    PowerSaving = 2,
    Custom30W = 3,
    Custom20W = 4,
    Custom10W = 5,
}

#[derive(Debug, Copy, Clone, Serialize_repr, Deserialize_repr, PartialEq)]
#[repr(u8)]
pub enum SensorDirection {
    /// Also used for clockwise in circular/arc progress & rotating pointer/dial indicator
    LeftToRight = 1,
    /// Also used for counter-clockwise in circular/arc & rotating pointer/dial progress indicator
    RightToLeft = 2,
    TopToBottom = 3,
    BottomToTop = 4,
}

/// Custom DIY panel definition
#[derive(Debug, Serialize, Deserialize)]
pub struct Panel {
    /// Custom panel id
    pub id: Option<String>,
    /// Custom panel name
    pub name: Option<String>,
    /*
    // The following fields of the AOOSTAR-X json configuration file are NOT used in `asterctl`
    /// TODO
    pub checked: Option<bool>,
    /// TODO panel type: 5 = built-in? 6 = custom ?
    #[serde(rename = "type")]
    pub panel_type: i32,
     */
    /// Background image filename
    pub img: Option<String>,
    /// Sensors
    pub sensor: Vec<Sensor>,
}

impl Panel {
    pub fn friendly_name(&self) -> String {
        self.name
            .clone()
            .or_else(|| self.id.clone())
            .or_else(|| {
                if let Some(img_file) = &self.img {
                    let img_file = PathBuf::from(img_file);
                    img_file
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "panel".into())
    }
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
    /// Label identifier, also used as data source identifier.
    pub label: String,
    /// Sensor value. Ignored: value is used from a sensor source
    #[serde(deserialize_with = "empty_string_as_none")]
    pub value: Option<String>, // "" or numbers, so Option<String>

    /// Image for progress, fan and pointer indicators
    pub min_value: Option<f32>,
    /// Image for progress, fan and pointer indicators
    pub max_value: Option<f32>,

    /// Optional unit text to print after the value
    #[serde(deserialize_with = "empty_string_as_none")]
    pub unit: Option<String>,
    /// x-position. Custom panel coordinates are stored as float!
    // TODO use i32 and round from f32 in deserialization
    pub x: f32,
    /// y-position.
    // TODO use i32 and round from f32 in deserialization
    pub y: f32,
    /// Used for pointer type
    pub width: Option<i32>,
    /// Used for pointer type
    pub height: Option<i32>,
    /// Sensor graphic orientation
    pub direction: Option<SensorDirection>,

    /// Font name matching font filename without file extension.
    pub font_family: String,
    /// TODO font size unit: points or pixels?
    pub font_size: i32,
    /// Font color in `#RRGGBB` notation, or -1 if not set. #ffffff = white, #ff0000 = red
    pub font_color: FontColor,
    /// _Not (yet) used_
    pub font_weight: FontWeight,
    pub text_align: TextAlign,

    /// Number of integer places for the sensor value.
    // -1 ≈ unset ⇒ Option<i32>
    #[serde(deserialize_with = "option_none_if_minus_one")]
    pub integer_digits: Option<i32>,
    /// Number of decimal places for the sensor value.
    // -1 ≈ unset ⇒ Option<i32>
    #[serde(deserialize_with = "option_none_if_minus_one")]
    pub decimal_digits: Option<i32>,
    /// Image for progress, fan and pointer indicators
    #[serde(deserialize_with = "empty_string_as_none")]
    pub pic: Option<String>,

    /// Used for fan & pointer sensors
    pub min_angle: Option<i32>,
    /// Used for fan & pointer sensors
    pub max_angle: Option<i32>,

    /// Pivot x
    #[serde(rename = "xz_x")]
    pub xz_x: Option<i32>,
    /// Pivot y
    #[serde(rename = "xz_y")]
    pub xz_y: Option<i32>,
    /*
    // The following fields of the AOOSTAR-X json configuration file are NOT used in `asterctl`
    /// _Not (yet) used_
    pub text_direction: i32, // layout direction
    /// For type = 6
    pub url: Option<String>,
    /// For type = 6
    pub data: Option<String>,
    /// For type = 6
    pub interval: Option<u32>,
     */
}

/// Sensor element type. Name is based on AOOSTAR-X web configuration
#[derive(Debug, Serialize_repr, Deserialize_repr, PartialEq)]
#[repr(u8)]
pub enum SensorMode {
    /// Text element
    Text = 1,
    /// Circular/arc progress indicator
    Fan = 2,
    /// Horizontal or vertical progress indicator
    Progress = 3,
    /// Rotating pointer/dial indicator
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

impl From<FontColor> for Rgba<u8> {
    fn from(val: FontColor) -> Self {
        Rgba([val.0[0], val.0[1], val.0[2], 255])
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
