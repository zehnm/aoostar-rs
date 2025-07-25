// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

mod cfg;
mod display;
mod font;
mod img;

use crate::cfg::Panel;
use crate::display::{AooScreen, AooScreenBuilder, DISPLAY_SIZE};
use crate::font::FontHandler;
use ab_glyph::{Font, PxScale};
use clap::Parser;
use env_logger::Env;
use image::imageops::FilterType;
use image::{ImageReader, Rgb, RgbImage};
use imageproc::drawing::{draw_line_segment_mut, draw_text_mut};
use log::{debug, info, warn};
use std::fs;
use std::io::Cursor;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// AOOSTAR WTR MAX and GEM12+ PRO screen control.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Serial device, for example "/dev/cu.usbserial-AB0KOHLS". Takes priority over --usb option.
    #[arg(short, long)]
    device: Option<String>,

    /// USB serial UART "vid:pid" in hex notation (lsusb output). Default: 416:90A1
    #[arg(short, long)]
    usb: Option<String>,

    /// Switch display on and exit. This will show the last displayed image.
    #[arg(long)]
    on: bool,

    /// Switch display off and exit.
    #[arg(long)]
    off: bool,

    /// Image to display, other sizes than 960x376 will be scaled.
    #[arg(short, long)]
    image: Option<String>,

    /// Run a demo
    #[arg(long)]
    demo: bool,

    /// Only for demo mode: AOOSTAR-X json configuration file to parse.
    #[arg(short, long)]
    config: Option<String>,

    /// Switch off display n seconds after loading image or running demo.
    #[arg(short, long)]
    off_after: Option<u32>,

    /// Test mode: only write to the display without checking response.
    #[arg(short, long)]
    write_only: bool,

    /// Test mode: save changed images in ./out folder.
    #[arg(short, long)]
    save: bool,
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let args = Args::parse();
    if let Some(config) = args.config.as_ref() {
        let _cfg = cfg::load_cfg(config)?;
    }

    // initialize display with given UART port parameter
    let mut builder = AooScreenBuilder::new();
    builder.no_init_check(args.write_only);
    let mut screen = if let Some(device) = args.device {
        builder.open_device(&device)?
    } else if let Some(usb) = args.usb {
        builder.open_usb_id(&usb)?
    } else {
        builder.open_default()?
    };

    // process simple commands
    if args.off {
        screen.off()?;
        return Ok(());
    } else if args.on {
        screen.on()?;
        return Ok(());
    }

    // switch on screen for remaining commands
    screen.init()?;

    if let Some(image) = args.image {
        info!("Loading and displaying background image {image}...");
        let rgb_img = img::load_image(&image, DISPLAY_SIZE)?;
        let timestamp = Instant::now();
        screen.send_image(&rgb_img)?;
        debug!("Image sent in {}ms", timestamp.elapsed().as_millis());
    }

    if args.demo {
        info!("Loading and displaying demo...");
        run_demo(&mut screen, args.config.as_deref(), args.save)?;
    }

    if let Some(off) = args.off_after {
        info!("Switching off display in {off}s");
        sleep(Duration::from_secs(off as u64));
        screen.off()?;
    }

    info!("Bye bye!");

    Ok(())
}

fn run_demo(screen: &mut AooScreen, config: Option<&str>, save_images: bool) -> anyhow::Result<()> {
    let rgb_img = demo_image()?;

    // fill left and right side of the loaded image with neighboring pixel color
    const WIDTH: u32 = 108;
    let rgb_img = demo_blinds(screen, &rgb_img, WIDTH, save_images)?;

    // print demo text over background image
    demo_text(screen, &rgb_img, save_images)?;

    if let Some(config) = config {
        let cfg = cfg::load_cfg(config)?;
        for active in cfg.active_panels.clone() {
            if active == 0 || active > cfg.panels.len() as u32 {
                warn!("Ignoring invalid active panel {active}");
                continue;
            }
            let panel = &cfg.panels[active as usize - 1];
            demo_panel(screen, &rgb_img, panel, save_images)?;
            break;
        }
    }

    Ok(())
}

fn demo_image() -> anyhow::Result<RgbImage> {
    let reader = ImageReader::new(Cursor::new(include_bytes!("../img/aybabtu.png")))
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
        // draw_antialiased_line_segment_mut(
        //     &mut rgb_img,
        //     (0, y as i32),
        //     (width as i32, y as i32),
        //     color,
        //     interpolate,
        // );
        draw_line_segment_mut(
            &mut rgb_img,
            (0.0, y as f32),
            (width as f32, y as f32),
            color,
        );
        let color = *rgb_img.get_pixel(DISPLAY_SIZE.0 - width - 1, y);
        // draw_antialiased_line_segment_mut(
        //     &mut rgb_img,
        //     ((DISPLAY_SIZE.0 - width) as i32, y as i32),
        //     (DISPLAY_SIZE.0 as i32, y as i32),
        //     color,
        //     interpolate,
        // );
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

fn demo_panel(
    screen: &mut AooScreen,
    background: &RgbImage,
    panel: &Panel,
    save_image: bool,
) -> anyhow::Result<()> {
    info!("Displaying panel information...");

    let mut rgb_img = background.clone();

    let mut fh = FontHandler::new("fonts");

    for sensor in &panel.sensor {
        println!(
            "({:03},{:03}): {}{}",
            sensor.x,
            sensor.y,
            sensor.value.as_deref().unwrap_or_default(),
            sensor.unit.as_deref().unwrap_or_default()
        );

        if let Some(value) = &sensor.value {
            let font = fh.get_ttf_font_or_default(&sensor.font_family);

            let text = format!("{value}{}", sensor.unit.as_deref().unwrap_or_default());
            let scale = font.pt_to_px_scale(sensor.font_size as f32).unwrap();
            draw_text_mut(
                &mut rgb_img,
                sensor.font_color.into(),
                // TODO figure out x,y unit conversion, something is off, probably in font scaling
                sensor.x as i32,
                sensor.y as i32,
                scale,
                &font,
                &text,
            );

            screen.send_image(&rgb_img)?;
        }
    }

    if save_image {
        fs::create_dir_all("out")?;
        rgb_img.save_with_format("out/panel.png", image::ImageFormat::Png)?;
    }

    Ok(())
}
