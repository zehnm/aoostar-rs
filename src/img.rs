// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

use bytes::{BufMut, BytesMut};
use image::imageops::FilterType;
use image::{GenericImageView, ImageReader, RgbImage};
use log::{debug, warn};
use std::path::Path;

pub fn load_image<P>(path: P, size: (u32, u32)) -> anyhow::Result<RgbImage>
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
        Ok(img
            .resize_exact(size.0, size.1, FilterType::Lanczos3)
            .to_rgb8())
    } else {
        Ok(img.to_rgb8())
    }
}

pub fn rgb888_to_565(rgb_img: &RgbImage) -> anyhow::Result<BytesMut> {
    let mut img_rgb565 =
        BytesMut::with_capacity(rgb_img.width() as usize * rgb_img.height() as usize * 2);

    for (_x, _y, pixel) in rgb_img.enumerate_pixels() {
        img_rgb565.put_u16_le(
            ((pixel.0[0] & 248) as u16) << 8
                | ((pixel.0[1] & 252) as u16) << 3
                | ((pixel.0[2] as u16) >> 3),
        );
    }
    Ok(img_rgb565)
}
