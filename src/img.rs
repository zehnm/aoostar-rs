// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

//! Image helper functions.

use bytes::{BufMut, BytesMut};
use image::imageops::FilterType;
use image::{
    DynamicImage, GenericImageView, ImageBuffer, ImageReader, ImageResult, RgbImage, Rgba,
    RgbaImage,
};
use imageproc::geometric_transformations::{Interpolation, rotate};
use log::{debug, warn};
use std::collections::HashMap;
use std::f32::consts::PI;
use std::path::{Path, PathBuf};

/// Width, height type
pub type Size = (u32, u32);

pub fn load_image<P>(path: P, size: Size) -> anyhow::Result<DynamicImage>
where
    P: AsRef<Path>,
{
    let img = ImageReader::open(path)?.decode()?;
    debug!(
        "Image dimensions: {:?}, {:?}",
        img.dimensions(),
        img.color()
    );

    if img.dimensions() != size {
        warn!(
            "Resizing invalid image dimensions {:?} to expected size {:?}, ignoring aspect ratio",
            img.dimensions(),
            size
        );
        Ok(img.resize_exact(size.0, size.1, FilterType::Lanczos3))
    } else {
        Ok(img)
    }
}

/// Trait definition to get a RGB 565 representation from a source image.
pub trait ToRgb565 {
    /// Get an RGB 565 representation of the image in little endian format.
    fn to_rgb565_le(&self) -> BytesMut;

    /// Convert a single RGB 888 pixel to 16 bit RGB 565 format.
    fn convert_rgb(&self, r: u8, g: u8, b: u8) -> u16 {
        ((r & 248) as u16) << 8 | ((g & 252) as u16) << 3 | ((b as u16) >> 3)
    }
}

// TODO quick & dirty approach for converting RgbImage & RgbaImage to RGB 565.
//      There should be a more generic way, maybe with PixelEnumerator...
impl ToRgb565 for &RgbImage {
    fn to_rgb565_le(&self) -> BytesMut {
        let mut img_rgb565 =
            BytesMut::with_capacity(self.width() as usize * self.height() as usize * 2);

        for (_x, _y, pixel) in self.enumerate_pixels() {
            img_rgb565.put_u16_le(self.convert_rgb(pixel.0[0], pixel.0[1], pixel.0[2]));
        }

        img_rgb565
    }
}

impl ToRgb565 for &RgbaImage {
    fn to_rgb565_le(&self) -> BytesMut {
        let mut img_rgb565 =
            BytesMut::with_capacity(self.width() as usize * self.height() as usize * 2);

        for (_x, _y, pixel) in self.enumerate_pixels() {
            img_rgb565.put_u16_le(self.convert_rgb(pixel.0[0], pixel.0[1], pixel.0[2]));
        }

        img_rgb565
    }
}

/// Cache for loaded images to avoid repeated file I/O
pub struct ImageCache {
    img_path: PathBuf,
    cache: HashMap<PathBuf, Option<RgbaImage>>,
}

impl ImageCache {
    pub fn new(img_path: impl Into<PathBuf>) -> Self {
        Self {
            img_path: img_path.into(),
            cache: HashMap::new(),
        }
    }

    /// Load and cache an image, returns None if loading fails
    pub fn get<P: AsRef<Path>>(&mut self, path: P) -> Option<&RgbaImage> {
        let path = path.as_ref();
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.img_path.join(path)
        };

        if !self.cache.contains_key(&path) {
            let image_result = match image::open(&path) {
                Ok(img) => Some(img.to_rgba8()),
                Err(e) => {
                    warn!("Failed to load image {:?}: {:?}", path, e);
                    None
                }
            };
            self.cache.insert(path.clone(), image_result);
        }

        self.cache.get(&path).and_then(|opt| opt.as_ref())
    }

    pub fn get_resized<P: AsRef<Path>>(&mut self, path: P, size: Size) -> Option<&RgbaImage> {
        let path = path.as_ref();
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.img_path.join(path)
        };

        if !self.cache.contains_key(&path) {
            let image = load_image(&path, size).ok()?.to_rgba8();
            self.cache.insert(path.clone(), Some(image));
        }

        self.cache.get(&path).and_then(|opt| opt.as_ref())
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

/// Quality settings for rotation
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum RotationQuality {
    /// Nearest neighbor
    Fast,
    /// Bilinear
    Good,
    /// Bicubic
    Best,
}

/// Rotate image by specified angle in degrees
pub fn rotate_image(image: &RgbaImage, angle_degrees: i32) -> RgbaImage {
    match angle_degrees {
        0 => image.clone(),
        90 => rotate_90_degrees(image, true),
        270 => rotate_90_degrees(image, false),
        180 => rotate_180_degrees(image),
        angle => {
            let angle_radians = angle as f32 * PI / 180.0;
            // TODO check Bilinear vs Bicubic
            rotate_about_center(image, angle_radians, RotationQuality::Good)
        }
    }
}

/// Rotate image about its center, maintaining original dimensions
fn rotate_about_center(
    image: &RgbaImage,
    angle_radians: f32,
    interpolation: RotationQuality,
) -> RgbaImage {
    let (width, height) = image.dimensions();
    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;

    let interp_method = match interpolation {
        RotationQuality::Fast => Interpolation::Nearest,
        RotationQuality::Good => Interpolation::Bilinear,
        RotationQuality::Best => Interpolation::Bicubic,
    };

    rotate(
        image,
        (center_x, center_y),
        angle_radians,
        interp_method,
        Rgba([0, 0, 0, 0]), // Transparent background for areas outside original image
    )
}

/// Fast 90-degree rotations (optimized for common cases)
pub fn rotate_90_degrees(image: &RgbaImage, clockwise: bool) -> RgbaImage {
    let (width, height) = image.dimensions();
    let mut rotated = ImageBuffer::new(height, width); // Swap dimensions

    if clockwise {
        for y in 0..height {
            for x in 0..width {
                let pixel = *image.get_pixel(x, y);
                rotated.put_pixel(height - 1 - y, x, pixel);
            }
        }
    } else {
        for y in 0..height {
            for x in 0..width {
                let pixel = *image.get_pixel(x, y);
                rotated.put_pixel(y, width - 1 - x, pixel);
            }
        }
    }

    rotated
}

/// Rotate by 180 degrees (optimized)
pub fn rotate_180_degrees(image: &RgbaImage) -> RgbaImage {
    let (width, height) = image.dimensions();
    let mut rotated = ImageBuffer::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let pixel = *image.get_pixel(x, y);
            rotated.put_pixel(width - 1 - x, height - 1 - y, pixel);
        }
    }

    rotated
}
