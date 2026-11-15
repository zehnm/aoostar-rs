// SPDX-License-Identifier: MIT OR Apache-2.0

//! Burn-in / image retention protection for long-running sensor panels.
//!
//! - Pixel shift: the whole frame slowly walks through small offsets, so static elements like
//!   headers, tile borders and captions don't stay on the same pixels.
//! - Wipe: periodically show the inverted frame for a few seconds to exercise all pixels.
//! - Off hours: switch the display off during a daily time window.

use anyhow::{Context, anyhow};
use chrono::{NaiveTime, Timelike};
use image::{Rgba, RgbaImage};
use std::str::FromStr;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct BurnInOptions {
    /// Move the frame to the next offset after this interval. `None` disables pixel shifting.
    pub pixel_shift_interval: Option<Duration>,
    /// Maximum offset in pixels in each direction.
    pub pixel_shift_max: u32,
    /// Show an inverted frame after this interval. `None` disables wiping.
    pub wipe_interval: Option<Duration>,
    /// Duration of the inverted frame.
    pub wipe_duration: Duration,
    /// Daily time window to switch off the display.
    pub off_hours: Option<OffHours>,
}

pub struct BurnInGuard {
    options: BurnInOptions,
    offsets: Vec<(i32, i32)>,
    start: Instant,
    last_wipe: Instant,
}

impl BurnInGuard {
    pub fn new(options: BurnInOptions) -> Self {
        let offsets = shift_offsets(options.pixel_shift_max as i32);
        let now = Instant::now();
        Self {
            options,
            offsets,
            start: now,
            last_wipe: now,
        }
    }

    /// Current pixel shift offset.
    pub fn offset(&self, now: Instant) -> (i32, i32) {
        match self.options.pixel_shift_interval {
            Some(interval) if !interval.is_zero() && self.offsets.len() > 1 => {
                let step = now.duration_since(self.start).as_secs() / interval.as_secs().max(1);
                self.offsets[step as usize % self.offsets.len()]
            }
            _ => (0, 0),
        }
    }

    /// Apply the current pixel shift to a rendered frame.
    pub fn apply(&self, image: RgbaImage, now: Instant) -> RgbaImage {
        let (dx, dy) = self.offset(now);
        if dx == 0 && dy == 0 {
            image
        } else {
            shift(&image, dx, dy)
        }
    }

    /// Check if a wipe is due. Resets the wipe timer if so.
    pub fn wipe_due(&mut self, now: Instant) -> bool {
        match self.options.wipe_interval {
            Some(interval) if now.duration_since(self.last_wipe) >= interval => {
                self.last_wipe = now;
                true
            }
            _ => false,
        }
    }

    pub fn wipe_duration(&self) -> Duration {
        self.options.wipe_duration
    }

    pub fn is_off_hours(&self, time: NaiveTime) -> bool {
        self.options
            .off_hours
            .as_ref()
            .is_some_and(|off| off.contains(time))
    }
}

/// All offsets within `[-max, max]²`, walked in a serpentine order so consecutive offsets differ
/// by a single pixel, starting at (0, 0).
fn shift_offsets(max: i32) -> Vec<(i32, i32)> {
    if max <= 0 {
        return vec![(0, 0)];
    }
    let mut offsets = Vec::new();
    for (row, dy) in (-max..=max).enumerate() {
        let xs: Vec<i32> = if row % 2 == 0 {
            (-max..=max).collect()
        } else {
            (-max..=max).rev().collect()
        };
        offsets.extend(xs.into_iter().map(|dx| (dx, dy)));
    }
    let center = offsets
        .iter()
        .position(|&o| o == (0, 0))
        .expect("center offset");
    offsets.rotate_left(center);
    offsets
}

/// Move the image by (dx, dy). Uncovered areas are black.
pub fn shift(image: &RgbaImage, dx: i32, dy: i32) -> RgbaImage {
    let (w, h) = image.dimensions();
    let mut shifted = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255]));
    for y in 0..h as i32 {
        let src_y = y - dy;
        if src_y < 0 || src_y >= h as i32 {
            continue;
        }
        for x in 0..w as i32 {
            let src_x = x - dx;
            if src_x >= 0 && src_x < w as i32 {
                shifted.put_pixel(
                    x as u32,
                    y as u32,
                    *image.get_pixel(src_x as u32, src_y as u32),
                );
            }
        }
    }
    shifted
}

/// Inverted frame for wiping.
pub fn invert(image: &RgbaImage) -> RgbaImage {
    let mut inverted = image.clone();
    for pixel in inverted.pixels_mut() {
        pixel[0] = 255 - pixel[0];
        pixel[1] = 255 - pixel[1];
        pixel[2] = 255 - pixel[2];
    }
    inverted
}

/// Daily time window, for example `23:00-07:00`. The window may span midnight.
#[derive(Debug, Clone, PartialEq)]
pub struct OffHours {
    pub from: NaiveTime,
    pub to: NaiveTime,
}

impl OffHours {
    pub fn contains(&self, time: NaiveTime) -> bool {
        let time =
            NaiveTime::from_hms_opt(time.hour(), time.minute(), time.second()).unwrap_or(time);
        if self.from <= self.to {
            time >= self.from && time < self.to
        } else {
            time >= self.from || time < self.to
        }
    }
}

impl FromStr for OffHours {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (from, to) = s
            .split_once('-')
            .ok_or_else(|| anyhow!("Expected HH:MM-HH:MM, got {s}"))?;
        let parse = |t: &str| {
            NaiveTime::parse_from_str(t.trim(), "%H:%M")
                .with_context(|| format!("Invalid time {t}, expected HH:MM"))
        };
        let off = OffHours {
            from: parse(from)?,
            to: parse(to)?,
        };
        if off.from == off.to {
            return Err(anyhow!("Off hours start and end must differ: {s}"));
        }
        Ok(off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn offsets_cover_square_and_start_centered() {
        let offsets = shift_offsets(2);
        assert_eq!(offsets.len(), 25);
        assert_eq!(offsets[0], (0, 0));
        for (dx, dy) in &offsets {
            assert!(dx.abs() <= 2 && dy.abs() <= 2);
        }
        assert_eq!(shift_offsets(0), vec![(0, 0)]);
    }

    #[test]
    fn consecutive_offsets_move_one_pixel_within_the_walk() {
        let offsets = shift_offsets(1);
        // the serpentine is continuous except where it wraps around
        let steps = offsets
            .windows(2)
            .filter(|w| (w[0].0 - w[1].0).abs() + (w[0].1 - w[1].1).abs() > 1)
            .count();
        assert!(steps <= 1);
    }

    #[test]
    fn offset_advances_with_interval() {
        let guard = BurnInGuard::new(BurnInOptions {
            pixel_shift_interval: Some(Duration::from_secs(60)),
            pixel_shift_max: 1,
            ..Default::default()
        });
        let start = guard.start;
        assert_eq!(guard.offset(start), (0, 0));
        assert_eq!(guard.offset(start + Duration::from_secs(59)), (0, 0));
        assert_ne!(guard.offset(start + Duration::from_secs(60)), (0, 0));
    }

    #[test]
    fn disabled_shift_keeps_frame() {
        let guard = BurnInGuard::new(BurnInOptions::default());
        assert_eq!(
            guard.offset(Instant::now() + Duration::from_secs(3600)),
            (0, 0)
        );
    }

    #[test]
    fn shift_moves_pixels_and_fills_black() {
        let mut img = RgbaImage::from_pixel(4, 3, Rgba([0, 0, 0, 255]));
        img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        let shifted = shift(&img, 2, 1);
        assert_eq!(*shifted.get_pixel(2, 1), Rgba([255, 0, 0, 255]));
        assert_eq!(*shifted.get_pixel(0, 0), Rgba([0, 0, 0, 255]));
        let back = shift(&shifted, -2, -1);
        assert_eq!(*back.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn invert_frame() {
        let img = RgbaImage::from_pixel(1, 1, Rgba([10, 200, 255, 255]));
        assert_eq!(*invert(&img).get_pixel(0, 0), Rgba([245, 55, 0, 255]));
    }

    #[test]
    fn wipe_timer() {
        let mut guard = BurnInGuard::new(BurnInOptions {
            wipe_interval: Some(Duration::from_secs(600)),
            ..Default::default()
        });
        let start = guard.last_wipe;
        assert!(!guard.wipe_due(start + Duration::from_secs(10)));
        assert!(guard.wipe_due(start + Duration::from_secs(600)));
        assert!(!guard.wipe_due(start + Duration::from_secs(601)));
    }

    #[test]
    fn off_hours_spanning_midnight() {
        let off: OffHours = "23:00-07:00".parse().unwrap();
        assert!(off.contains(t(23, 0)));
        assert!(off.contains(t(2, 30)));
        assert!(!off.contains(t(7, 0)));
        assert!(!off.contains(t(12, 0)));
    }

    #[test]
    fn off_hours_same_day() {
        let off: OffHours = "09:00-17:30".parse().unwrap();
        assert!(off.contains(t(9, 0)));
        assert!(!off.contains(t(17, 30)));
        assert!(!off.contains(t(8, 59)));
    }

    #[test]
    fn off_hours_parse_errors() {
        assert!("23:00".parse::<OffHours>().is_err());
        assert!("25:00-07:00".parse::<OffHours>().is_err());
        assert!("07:00-07:00".parse::<OffHours>().is_err());
    }
}
