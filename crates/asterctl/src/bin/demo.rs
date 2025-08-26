// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

use asterctl::cfg;
use asterctl::font::FontHandler;
use asterctl::render::PanelRenderer;
use asterctl_lcd::{AooScreen, AooScreenBuilder, DISPLAY_SIZE};

use ab_glyph::PxScale;
use clap::Parser;
use env_logger::Env;
use image::imageops::FilterType;
use image::{ImageReader, Rgb, RgbImage};
use imageproc::drawing::{draw_line_segment_mut, draw_text_mut};
use log::{error, info};
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// AOOSTAR WTR MAX and GEM12+ PRO screen control demo.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Serial device, for example "/dev/cu.usbserial-AB0KOHLS". Takes priority over --usb option.
    #[arg(short, long)]
    device: Option<String>,

    /// USB serial UART "vid:pid" in hex notation (lsusb output). Default: 416:90A1
    #[arg(short, long)]
    usb: Option<String>,

    /// AOOSTAR-X json configuration file to parse.
    ///
    /// The configuration file will be loaded from the `config_dir` directory if no full path is
    /// specified.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Configuration directory containing configuration files and background images
    /// specified in the `config` file. Default: `./cfg`
    #[arg(long)]
    config_dir: Option<PathBuf>,

    /// Font directory for fonts specified in the `config` file. Default: `./fonts`
    #[arg(long)]
    font_dir: Option<PathBuf>,

    /// Switch off display n seconds after loading image.
    #[arg(short, long)]
    off_after: Option<u32>,

    /// Test mode: only write to the display without checking response.
    #[arg(short, long)]
    write_only: bool,

    /// Test mode: save changed images in ./out folder.
    #[arg(short, long)]
    save: bool,

    /// Simulate serial port for testing and development, `--device` and `--usb` options are ignored.
    #[arg(long)]
    simulate: bool,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    // initialize display with given UART port parameter
    let mut builder = AooScreenBuilder::new();
    builder.no_init_check(args.write_only);
    let mut screen = if args.simulate {
        builder.simulate()?
    } else if let Some(device) = args.device {
        builder.open_device(&device)?
    } else if let Some(usb) = args.usb {
        builder.open_usb_id(&usb)?
    } else {
        builder.open_default()?
    };

    info!("Loading and displaying demo...");
    run_demo(
        &mut screen,
        args.config.as_deref(),
        args.config_dir.unwrap_or_else(|| "cfg".into()),
        args.font_dir.unwrap_or_else(|| "fonts".into()),
        args.save,
    )?;

    if let Some(off) = args.off_after {
        info!("Switching off display in {off}s");
        sleep(Duration::from_secs(off as u64));
        screen.off()?;
    }

    info!("Bye bye!");
    Ok(())
}

fn run_demo(
    screen: &mut AooScreen,
    config: Option<&Path>,
    config_dir: PathBuf,
    font_dir: PathBuf,
    save_images: bool,
) -> anyhow::Result<()> {
    let rgb_img = demo_image()?;

    // fill left and right side of the loaded image with neighboring pixel color
    const WIDTH: u32 = 108;
    let rgb_img = demo_blinds(screen, &rgb_img, WIDTH, save_images)?;

    // print demo text over background image
    demo_text(screen, &rgb_img, save_images)?;

    if let Some(config) = config {
        let mut cfg = if config.is_absolute() {
            cfg::load_cfg(config)?
        } else {
            cfg::load_cfg(config_dir.join(config))?
        };

        if let Some(panel) = cfg.get_next_active_panel() {
            info!("Displaying demo panel...");

            // get sensor values from panel configuration
            let mut demo_values = HashMap::new();
            for sensor in &panel.sensor {
                demo_values.insert(
                    sensor.label.clone(),
                    sensor.value.clone().unwrap_or_default(),
                );
            }

            let mut renderer = PanelRenderer::new(DISPLAY_SIZE, &font_dir, &config_dir);
            renderer.set_save_render_img(save_images);
            renderer.set_save_processed_pic(save_images);
            renderer.set_save_progress_layer(save_images);

            match renderer.render(panel, &demo_values) {
                Ok(image) => screen.send_image(&image)?,
                Err(e) => error!("Error rendering panel '{}': {e:?}", panel.friendly_name()),
            }
        } else {
            error!("No active panel found");
        }
    }

    Ok(())
}

fn demo_image() -> anyhow::Result<RgbImage> {
    let reader = ImageReader::new(Cursor::new(include_bytes!("aybabtu.png")))
        .with_guessed_format()
        .expect("Cursor io never fails");

    Ok(reader
        .decode()?
        .resize_exact(DISPLAY_SIZE.0, DISPLAY_SIZE.1, FilterType::Lanczos3)
        .to_rgb8())
}

fn demo_text(
    screen: &mut AooScreen,
    background: &RgbImage,
    save_images: bool,
) -> anyhow::Result<()> {
    let text = "ALL YOUR BASE ARE BELONG TO US.";
    let text_upd_delay = Duration::from_millis(0);
    let font = FontHandler::default_font();
    let height = 36.0;
    let scale = PxScale {
        x: height,
        y: height,
    };

    if save_images {
        fs::create_dir_all("out")?;
    }

    for text_idx in 0..text.len() {
        info!("Printing: {}", &text[0..text_idx + 1]);
        let text_upd = Instant::now();
        let mut rgb_img = background.clone();
        draw_text_mut(
            &mut rgb_img,
            Rgb([118u8, 118u8, 97u8]),
            4 * 47,
            300,
            scale,
            &font,
            &text[0..text_idx + 1],
        );

        if save_images {
            rgb_img.save_with_format(
                format!("out/demo_text-{text_idx}.png"),
                image::ImageFormat::Png,
            )?;
        }

        screen.send_image(&rgb_img)?;

        let elapsed = text_upd.elapsed();
        if elapsed < text_upd_delay {
            sleep(text_upd_delay - elapsed);
        }
    }

    Ok(())
}

// CPU intensive! Release build is ~ 5x faster on M1 Max
fn demo_blinds(
    screen: &mut AooScreen,
    background: &RgbImage,
    width: u32,
    save_images: bool,
) -> anyhow::Result<RgbImage> {
    let mut rgb_img = background.clone();

    info!("Masking {width} pixels of left & right image...");

    if save_images {
        fs::create_dir_all("out")?;
    }

    for y in 0..DISPLAY_SIZE.1 {
        let color = *rgb_img.get_pixel(width + 1, y);
        draw_line_segment_mut(
            &mut rgb_img,
            (0.0, y as f32),
            (width as f32, y as f32),
            color,
        );
        let color = *rgb_img.get_pixel(DISPLAY_SIZE.0 - width - 1, y);
        draw_line_segment_mut(
            &mut rgb_img,
            ((DISPLAY_SIZE.0 - width) as f32, y as f32),
            (DISPLAY_SIZE.0 as f32, y as f32),
            color,
        );

        if y % 5 == 0 {
            screen.send_image(&rgb_img)?;
        }

        if save_images {
            rgb_img
                .save_with_format(format!("out/demo_blinds-{y}.png"), image::ImageFormat::Png)?;
        }
    }

    screen.send_image(&rgb_img)?;

    Ok(rgb_img)
}
