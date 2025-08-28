// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

#![forbid(non_ascii_idents)]
#![deny(unsafe_code)]

use bytes::{BufMut, BytesMut};
use image::{RgbImage, RgbaImage};

mod aoo_screen;
mod fake_serialport;

pub use aoo_screen::{AooScreen, AooScreenBuilder, DISPLAY_SIZE};
pub use fake_serialport::FakeSerialPort;

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
