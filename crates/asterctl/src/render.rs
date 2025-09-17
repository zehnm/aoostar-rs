// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

//! Sensor panel rendering logic. Create an RGBa image from a panel configuration and sensor values.

use crate::cfg::{Panel, Sensor, SensorDirection, SensorMode, TextAlign};
use crate::font::FontHandler;
use crate::format_value;
use crate::img::{ImageCache, Size, rotate_image};
use crate::sensors::get_date_time_value;
use ab_glyph::Font;
use chrono::{DateTime, Local};
use image::{ImageBuffer, Rgba, RgbaImage};
use imageproc::drawing::{draw_text_mut, text_size};
use log::{debug, error};
use std::collections::HashMap;
use std::f32::consts::PI;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

/// Error type for image processing operations
#[derive(Debug)]
#[allow(dead_code)]
pub enum ImageProcessingError {
    ImageLoadError(String),
    InvalidMode(i32),
    InvalidDirection(SensorDirection),
    MathError(String),
    IoError(std::io::Error),
}

impl From<std::io::Error> for ImageProcessingError {
    fn from(error: std::io::Error) -> Self {
        ImageProcessingError::IoError(error)
    }
}

/// Sensor panel renderer.
///
/// Renders a final display image from a sensor panel configuration and current sensor values.
/// All defined fonts and images of a sensor panel are cached after first use.
pub struct PanelRenderer {
    size: Size,
    composite_layer_map: HashMap<SensorMode, RgbaImage>,
    font_handler: FontHandler,
    image_cache: ImageCache,
    // for debugging: save images for inspection
    save_render_img: bool,
    save_processed_pic: bool,
    save_progress_layer: bool,
    img_save_path: PathBuf,
    img_suffix: Option<String>,
}

impl PanelRenderer {
    /// Create a new image processor instance for a given display size.
    ///
    /// # Arguments
    ///
    /// * `size`: display size, used to render a panel image.
    /// * `font_dir`: font directory to load TTF fonts specified in a sensor configuration.
    /// * `img_dir`: image directory to load background and sensor images from.
    ///
    /// returns: PanelRenderer
    pub fn new(size: Size, font_dir: impl Into<PathBuf>, img_dir: impl Into<PathBuf>) -> Self {
        Self {
            size,
            composite_layer_map: HashMap::new(),
            font_handler: FontHandler::new(font_dir),
            image_cache: ImageCache::new(img_dir),
            save_render_img: false,
            save_processed_pic: false,
            save_progress_layer: false,
            img_save_path: PathBuf::from("out"),
            img_suffix: None,
        }
    }

    /// For debugging: save rendered panel image as .PNG graphic for inspection.
    pub fn set_save_render_img(&mut self, save: bool) {
        self.save_render_img = save;
        self.create_img_save_path();
    }
    /// For debugging: save all processed sensor pic images as .PNG graphics for inspection.
    pub fn set_save_processed_pic(&mut self, save: bool) {
        self.save_processed_pic = save;
        self.create_img_save_path();
    }
    /// For debugging: save all progress layer images as .PNG graphics for inspection.
    pub fn set_save_progress_layer(&mut self, save: bool) {
        self.save_progress_layer = save;
        self.create_img_save_path();
    }
    /// Set output directory path for saving images.
    ///
    /// Default output directory is `./out` in the current working directory.
    pub fn set_img_save_path(&mut self, img_dir: impl Into<PathBuf>) {
        self.img_save_path = img_dir.into();
        self.create_img_save_path();
    }
    /// Set an optional image name suffix for saving a .PNG graphic file.
    ///
    /// This function needs to be called before [render()] if a different suffix should be used for each rendered panel.
    pub fn set_img_suffix(&mut self, img_suffix: impl Into<String>) {
        self.img_suffix = Some(img_suffix.into());
    }

    /// Render a sensor panel with the given values and return the final panel image.
    ///
    /// # Arguments
    ///
    /// * `panel`: the panel configuration
    /// * `values`: current values for the defined panel sensors in a shared HashMap
    ///
    /// returns: a rendered panel image in [RgbaImage] format, or an [ImageProcessingError] in case of an error.
    pub fn render(
        &mut self,
        panel: &Panel,
        values: &HashMap<String, String>,
    ) -> Result<RgbaImage, ImageProcessingError> {
        debug!(
            "Rendering panel {}...",
            panel
                .name
                .as_deref()
                .unwrap_or_else(|| panel.id.as_deref().unwrap_or_default())
        );

        let now = Instant::now();
        let background = if let Some(img) = &panel.img
            && let Some(background) = self.image_cache.get(img, Some(self.size))
        {
            background.clone()
        } else {
            RgbaImage::new(self.size.0, self.size.1)
        };
        self.composite_layer_map.clear();

        let final_image = self.render_all_sensors(panel, values, background)?;

        debug!("Rendered panel in {}ms", now.elapsed().as_millis());

        if self.save_render_img {
            let name = format!(
                "render_{}{}.png",
                panel.friendly_name(),
                self.img_suffix.as_deref().unwrap_or_default()
            );
            if let Err(e) = final_image.save(self.img_save_path.join(name)) {
                error!("Error saving rendered panel image: {e}");
            }
        }

        Ok(final_image)
    }

    /// Render all panel sensors with the given values on a background image
    pub fn render_all_sensors(
        &mut self,
        panel: &Panel,
        values: &HashMap<String, String>,
        mut background: RgbaImage,
    ) -> Result<RgbaImage, ImageProcessingError> {
        let now: DateTime<Local> = Local::now();

        for sensor in &panel.sensor {
            let value = values.get(&sensor.label).cloned();
            let unit = values
                .get(&format!("{}#unit", sensor.label))
                .cloned()
                .or_else(|| sensor.unit.clone())
                .unwrap_or_default();

            if let Some(value) = value {
                self.render_sensor(&mut background, sensor, &value, &unit)?;
            } else if let Some(value) = get_date_time_value(&sensor.label, &now) {
                self.render_sensor(&mut background, sensor, &value, &unit)?;
            }
        }

        // Final compositing
        self.composite_layers(&mut background);

        Ok(background)
    }

    /// Render a single sensor element based on its mode
    fn render_sensor(
        &mut self,
        background: &mut RgbaImage,
        sensor: &Sensor,
        value: &str,
        unit: &str,
    ) -> Result<(), ImageProcessingError> {
        let direction = sensor.direction.unwrap_or(SensorDirection::LeftToRight);

        match sensor.mode {
            SensorMode::Text => self.render_text(background, sensor, value, unit),
            SensorMode::Fan => self.render_fan(sensor, value, direction),
            SensorMode::Progress => self.render_progress(sensor, value, direction),
            SensorMode::Pointer => self.render_pointer(sensor, value, direction),
        }
    }

    /// Mode 1 - Text
    fn render_text(
        &mut self,
        background: &mut RgbaImage,
        sensor: &Sensor,
        value: &str,
        unit: &str,
    ) -> Result<(), ImageProcessingError> {
        let font = if let Some(font_family) = &sensor.font_family {
            self.font_handler.get_ttf_font_or_default(font_family)
        } else {
            FontHandler::default_font()
        };
        let font_size = sensor.font_size.unwrap_or(14) as f32;
        // TODO verify pixel scaling! Is font_size point size or pixel size?
        // TODO some font size calculation is missing, dpi scaling? internal padding?
        //      The adjustment hack is required to get the correct size of the rendered text.
        //      However, the y-position requires the regular value (see multiplication by 1.33 below)
        let adjustment_hack = 0.75;
        let scale = font.pt_to_px_scale(font_size * adjustment_hack).unwrap();

        let text = format_value(
            value,
            sensor.integer_digits.into(),
            sensor.decimal_digits.unwrap_or_default() as usize,
            unit,
        );
        let size = text_size(scale, &font, &text);
        let width = sensor.width.unwrap_or_default() as i32;
        let height = sensor.height.unwrap_or_default() as i32;
        let x = match sensor.text_align.unwrap_or_default() {
            TextAlign::Left => sensor.x,
            TextAlign::Center => sensor.x + width / 2 - (size.0 / 2) as i32,
            TextAlign::Right => sensor.x + width - size.0 as i32,
        };
        // FIXME figure out font scaling factor / padding / dpi etc. See above for y-adjustment hack.
        // This work quite ok for most panels, but not all!
        // Some work better with `sensor.y + height / 2 - size.1 as i32;`
        // The y parameter in `draw_text_mut` is still a mystery: drawing text at position (0,0)
        // renders a huge gap at the top, about the size of half the font-height!?
        let y = sensor.y + height / 2 - (size.1 as f32 * 1.3333 / 2f32) as i32;

        debug!(
            "Sensor({:03},{:03}), pixel({x:03},{y:03}), size{size:?}: {text}",
            sensor.x, sensor.y
        );

        let font_color = sensor.font_color.unwrap_or_default().into();
        draw_text_mut(background, font_color, x, y, scale, &font, &text);

        Ok(())
    }

    /// Mode 2 - Circular/Arc progress indicator
    fn render_fan(
        &mut self,
        sensor: &Sensor,
        value: &str,
        direction: SensorDirection,
    ) -> Result<(), ImageProcessingError> {
        if !matches!(
            direction,
            SensorDirection::LeftToRight | SensorDirection::RightToLeft
        ) {
            return Err(ImageProcessingError::InvalidDirection(direction));
        }

        let pos_x = sensor.x;
        let pos_y = sensor.y;

        let pic_path = sensor.pic.as_ref().ok_or_else(|| {
            ImageProcessingError::ImageLoadError("No picture specified".to_string())
        })?;

        let target_image = self
            .image_cache
            .get(pic_path, None)
            .ok_or_else(|| {
                ImageProcessingError::ImageLoadError(format!("Failed to load: {:?}", pic_path))
            })?
            .clone();

        let min_angle = sensor.min_angle.unwrap_or(0) as f32;
        let max_angle = sensor.max_angle.unwrap_or(180) as f32;
        let min_value = sensor.min_value.unwrap_or(0.0);
        let max_value = sensor.max_value.unwrap_or(100.0);

        let current_value = value
            .parse::<f32>()
            .map_err(|_| ImageProcessingError::MathError("Invalid value".to_string()))?;

        if current_value <= min_value {
            return Ok(());
        }

        let progress = if current_value >= max_value {
            1.0
        } else {
            (current_value - min_value) / (max_value - min_value)
        };

        let (start_angle, end_angle) = if direction == SensorDirection::LeftToRight {
            // Clockwise
            let start = min_angle - 90.0;
            let end = min_angle + (max_angle - min_angle) * progress - 90.0;
            (start, end)
        } else {
            // Counter-clockwise
            let start = 360.0 - min_angle - (max_angle - min_angle) * progress - 90.0;
            let end = 360.0 - min_angle - 90.0;
            (start, end)
        };

        if let Some(sector_layer) = self.get_layer(SensorMode::Fan) {
            PanelRenderer::draw_pie_slice(
                sector_layer,
                &target_image,
                pos_x,
                pos_y,
                start_angle,
                end_angle,
            );
        }

        Ok(())
    }

    /// Mode 3 - render progress graphic based on percentage value.
    ///
    /// The progress graphic must show the 100% value and is cut based on the actual value.
    fn render_progress(
        &mut self,
        sensor: &Sensor,
        value: &str,
        direction: SensorDirection,
    ) -> Result<(), ImageProcessingError> {
        let pic_path = sensor.pic.as_ref().ok_or_else(|| {
            ImageProcessingError::ImageLoadError("No picture specified".to_string())
        })?;

        let mut processed_img = self
            .image_cache
            .get(pic_path, None)
            .ok_or_else(|| {
                ImageProcessingError::ImageLoadError(format!("Failed to load: {:?}", pic_path))
            })?
            .clone();

        let min_val = sensor.min_value.unwrap_or(0.0);
        let max_val = sensor.max_value.unwrap_or(100.0);

        let current_value = value
            .parse::<f32>()
            .map_err(|_| ImageProcessingError::MathError("Invalid value".to_string()))?;

        let clamped_value = current_value.clamp(min_val, max_val);
        let progress = ((clamped_value - min_val) / (max_val - min_val)).clamp(0.0, 1.0);

        let (img_w, img_h) = processed_img.dimensions();

        // Create progress mask based on direction
        let crop_rect = match direction {
            SensorDirection::LeftToRight => {
                let crop_w = (img_w as f32 * progress).round() as u32;
                (0, 0, crop_w, img_h)
            }
            SensorDirection::RightToLeft => {
                let crop_w = (img_w as f32 * progress).round() as u32;
                (img_w - crop_w, 0, img_w, img_h)
            }
            SensorDirection::TopToBottom => {
                let crop_h = (img_h as f32 * progress).round() as u32;
                (0, 0, img_w, crop_h)
            }
            SensorDirection::BottomToTop => {
                let crop_h = (img_h as f32 * progress).round() as u32;
                (0, img_h - crop_h, img_w, img_h)
            }
        };

        // Apply crop mask to image
        self.apply_progress_mask(&mut processed_img, crop_rect, direction);

        if self.save_processed_pic {
            let name = format!(
                "processed_img-{}{}.png",
                sensor.label,
                self.img_suffix.as_deref().unwrap_or_default()
            );
            if let Err(e) = processed_img.save(self.img_save_path.join(name)) {
                error!("Error saving processed image: {e}");
            }
        }

        let pos_x = sensor.x;
        let pos_y = sensor.y;

        if let Some(progress_layer) = self.get_layer(SensorMode::Progress) {
            PanelRenderer::paste_image(progress_layer, &processed_img, pos_x, pos_y);

            if self.save_progress_layer {
                let name = format!(
                    "progress_layer-{}{}.png",
                    sensor.label,
                    self.img_suffix.as_deref().unwrap_or_default()
                );
                if let Err(e) = processed_img.save(self.img_save_path.join(name)) {
                    error!("Error saving progress layer image: {e}");
                }
            }
        }
        Ok(())
    }

    /// Mode 4 - Rotating pointer/dial indicator
    /// TODO needs testing
    fn render_pointer(
        &mut self,
        sensor: &Sensor,
        value: &str,
        direction: SensorDirection,
    ) -> Result<(), ImageProcessingError> {
        if !matches!(
            direction,
            SensorDirection::LeftToRight | SensorDirection::RightToLeft
        ) {
            return Err(ImageProcessingError::InvalidDirection(direction));
        }

        let x_center = sensor.x;
        let y_center = sensor.y;
        let xz_x = sensor.xz_x.unwrap_or(0);
        let xz_y = sensor.xz_y.unwrap_or(0);

        let pic_path = sensor.pic.as_ref().ok_or_else(|| {
            ImageProcessingError::ImageLoadError("No picture specified".to_string())
        })?;

        // Resize if dimensions specified
        let size = if let (Some(width), Some(height)) = (sensor.width, sensor.height) {
            Some((width, height))
        } else {
            None
        };
        let pic = self
            .image_cache
            .get(pic_path, size)
            .ok_or_else(|| {
                ImageProcessingError::ImageLoadError(format!("Failed to load: {:?}", pic_path))
            })?
            .clone();

        let min_val = sensor.min_value.unwrap_or(0.0);
        let max_val = sensor.max_value.unwrap_or(100.0);
        let current_value = value
            .parse::<f32>()
            .map_err(|_| ImageProcessingError::MathError("Invalid value".to_string()))?;

        let clamped_value = current_value.clamp(min_val, max_val);

        // Calculate progress
        let progress = if (max_val - min_val).abs() < f32::EPSILON {
            0.0
        } else {
            (clamped_value - min_val) / (max_val - min_val)
        };

        let mut min_angle = sensor.min_angle.unwrap_or(0) as f32;
        let mut max_angle = sensor.max_angle.unwrap_or(360) as f32;

        // Adjust angles for counter-clockwise
        if direction == SensorDirection::RightToLeft {
            min_angle = -min_angle;
            max_angle = -max_angle;
        }

        let angle = min_angle + progress * (max_angle - min_angle);
        let angle_rad = angle.to_radians();

        // Calculate offset based on rotation
        let offset_x = (xz_x as f32 * angle_rad.cos() - xz_y as f32 * angle_rad.sin()) as i32;
        let offset_y = (xz_x as f32 * angle_rad.sin() + xz_y as f32 * angle_rad.cos()) as i32;

        // Rotate the image
        let angle = angle.round() as i32;
        let rotated_pic = rotate_image(&pic, -angle);

        // Calculate final position
        let final_x = x_center + offset_x - (rotated_pic.width() / 2) as i32;
        let final_y = y_center + offset_y - (rotated_pic.height() / 2) as i32;

        if let Some(pointer_layer) = self.get_layer(SensorMode::Pointer) {
            PanelRenderer::paste_image(pointer_layer, &rotated_pic, final_x, final_y);
        }
        Ok(())
    }

    /// Draws a pie‐slice sector of the `source` image into the `layer` destination.
    ///
    /// Pixels in the sector are alpha-blended from source into the destination layer at the given
    /// center_x/center_y placement.
    ///
    /// Positive and negative angles are supported and are automatically normalized if > +/- 360°.
    ///
    /// # Arguments
    ///
    /// * `layer`: Destination layer.
    /// * `source`: Source image to cut out a pie-slice sector.
    /// * `center_x`: Center x position.
    /// * `center_y`: Center y position.
    /// * `start_deg`: Starting angle, in degrees. Angles are measured from 3 o’clock, increasing clockwise.
    /// * `end_deg`: Ending angle, in degrees.
    ///
    fn draw_pie_slice(
        layer: &mut RgbaImage,
        source: &RgbaImage,
        center_x: i32,
        center_y: i32,
        start_deg: f32,
        end_deg: f32,
    ) {
        let (src_w, src_h) = source.dimensions();
        // Radius is half the smaller dimension
        let radius = (src_w.min(src_h) as f32) / 2.0;
        // Convert angles to radians and normalize
        let start = (start_deg % 360f32).to_radians();
        let end = (end_deg % 360f32).to_radians();
        // Helper: check if angle t is between start and end (clockwise)
        let in_sector = |t: f32| {
            let mut a = t;
            if a < 0.0 {
                a += 2.0 * PI;
            }
            let mut s = start;
            let mut e = end;
            if s < 0.0 {
                s += 2.0 * PI;
            }
            if e < 0.0 {
                e += 2.0 * PI;
            }
            if e < s {
                // wrap
                a >= s || a <= e
            } else {
                a >= s && a <= e
            }
        };

        for sy in 0..src_h {
            for sx in 0..src_w {
                // Coordinates relative to center of source
                let dx = sx as f32 - src_w as f32 / 2.0;
                let dy = sy as f32 - src_h as f32 / 2.0;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= radius {
                    // Polar angle (atan2 returns [-PI, PI], 0 at +x axis)
                    let angle = dy.atan2(dx);
                    if in_sector(angle) {
                        // Pixel is inside the slice: blend it into layer
                        let dest_x = center_x + sx as i32 - src_w as i32 / 2;
                        let dest_y = center_y + sy as i32 - src_h as i32 / 2;
                        if dest_x >= 0 && dest_y >= 0 {
                            let (lw, lh) = layer.dimensions();
                            if (dest_x as u32) < lw && (dest_y as u32) < lh {
                                let src_px = source.get_pixel(sx, sy);
                                let dst_px = layer.get_pixel_mut(dest_x as u32, dest_y as u32);
                                // alpha‐blend: out = src.a*src + (1−src.a)*dst
                                let alpha = src_px[3] as f32 / 255.0;
                                for i in 0..3 {
                                    dst_px[i] = ((src_px[i] as f32 * alpha)
                                        + (dst_px[i] as f32 * (1.0 - alpha)))
                                        .round()
                                        as u8;
                                }
                                for i in 0..4 {
                                    dst_px[i] = ((src_px[i] as f32 * alpha)
                                        + (dst_px[i] as f32 * (1.0 - alpha)))
                                        .round()
                                        as u8;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Apply progress mask to image based on crop rectangle and direction
    fn apply_progress_mask(
        &self,
        image: &mut RgbaImage,
        crop_rect: (u32, u32, u32, u32),
        direction: SensorDirection,
    ) {
        let (crop_x, crop_y, crop_w, crop_h) = crop_rect;
        let (img_w, img_h) = image.dimensions();

        // Create mask - set alpha to 0 outside crop area
        for y in 0..img_h {
            for x in 0..img_w {
                let should_keep = match direction {
                    SensorDirection::LeftToRight => x < crop_w,
                    SensorDirection::RightToLeft => x >= crop_x,
                    SensorDirection::TopToBottom => y < crop_h,
                    SensorDirection::BottomToTop => y >= crop_y,
                };

                if !should_keep {
                    let pixel = image.get_pixel_mut(x, y);
                    pixel[3] = 0; // Set alpha to 0 (transparent)
                }
            }
        }
    }

    /// Paste an image onto another image at specified position
    fn paste_image(target: &mut RgbaImage, source: &RgbaImage, x: i32, y: i32) {
        let (target_w, target_h) = target.dimensions();
        let (source_w, source_h) = source.dimensions();

        for sy in 0..source_h {
            for sx in 0..source_w {
                let target_x = x + sx as i32;
                let target_y = y + sy as i32;

                if target_x >= 0
                    && target_y >= 0
                    && (target_x as u32) < target_w
                    && (target_y as u32) < target_h
                {
                    let source_pixel = *source.get_pixel(sx, sy);
                    let target_pixel = target.get_pixel_mut(target_x as u32, target_y as u32);

                    // Alpha blending
                    let alpha = source_pixel[3] as f32 / 255.0;
                    let inv_alpha = 1.0 - alpha;

                    for i in 0..3 {
                        target_pixel[i] = ((source_pixel[i] as f32 * alpha)
                            + (target_pixel[i] as f32 * inv_alpha))
                            as u8;
                    }
                    target_pixel[3] = ((source_pixel[3] as f32 * alpha)
                        + (target_pixel[3] as f32 * inv_alpha))
                        as u8;
                }
            }
        }
    }

    fn create_img_save_path(&mut self) {
        if (self.save_render_img || self.save_processed_pic || self.save_progress_layer)
            && let Err(e) = fs::create_dir_all(&self.img_save_path)
        {
            error!(
                "Error creating image output path {:?}: {e}",
                self.img_save_path
            );
        }
    }

    fn get_layer(&mut self, mode: SensorMode) -> Option<&mut RgbaImage> {
        if !self.composite_layer_map.contains_key(&mode) {
            self.composite_layer_map.insert(mode, self.create_layer());
        }

        self.composite_layer_map.get_mut(&mode)
    }

    /// Create an overlay image buffer with the same dimensions as the panel
    fn create_layer(&self) -> RgbaImage {
        ImageBuffer::from_fn(self.size.0, self.size.1, |_, _| Rgba([0, 0, 0, 0]))
    }

    /// Composite all layers into final image
    fn composite_layers(&mut self, background: &mut RgbaImage) {
        // quick and dirty, this should be an ordered enum variant list
        let modes = [SensorMode::Fan, SensorMode::Progress, SensorMode::Pointer];
        for mode in modes {
            if let Some(layer) = self.composite_layer_map.get(&mode) {
                // Find bounding box of non-transparent pixels
                let bbox = PanelRenderer::get_bounding_box(layer);

                if let Some((min_x, min_y, max_x, max_y)) = bbox {
                    // Composite the layer onto final image
                    for y in min_y..=max_y {
                        for x in min_x..=max_x {
                            let layer_pixel = *layer.get_pixel(x, y);
                            if layer_pixel[3] > 0 {
                                // If not fully transparent
                                let final_pixel = background.get_pixel_mut(x, y);

                                // Alpha compositing
                                let alpha = layer_pixel[3] as f32 / 255.0;
                                let inv_alpha = 1.0 - alpha;

                                for i in 0..4 {
                                    final_pixel[i] = ((layer_pixel[i] as f32 * alpha)
                                        + (final_pixel[i] as f32 * inv_alpha))
                                        as u8;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get bounding box of non-transparent pixels
    fn get_bounding_box(image: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
        let (width, height) = image.dimensions();
        let mut min_x = width;
        let mut min_y = height;
        let mut max_x = 0;
        let mut max_y = 0;
        let mut found_pixel = false;

        for y in 0..height {
            for x in 0..width {
                let pixel = image.get_pixel(x, y);
                if pixel[3] > 0 {
                    // Non-transparent
                    found_pixel = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }

        if found_pixel {
            Some((min_x, min_y, max_x, max_y))
        } else {
            None
        }
    }
}
