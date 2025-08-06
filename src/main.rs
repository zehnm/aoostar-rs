// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

mod cfg;
mod display;
mod font;
mod img;
mod sensors;

use crate::cfg::{MonitorConfig, Panel, SensorMode, TextAlign};
use crate::display::{AooScreen, AooScreenBuilder, DISPLAY_SIZE};
use crate::font::FontHandler;
use crate::sensors::start_file_slurper;
use ab_glyph::{Font, PxScale};
use anyhow::anyhow;
use clap::Parser;
use env_logger::Env;
use image::imageops::FilterType;
use image::{ImageReader, Rgb, RgbImage};
use imageproc::drawing::{draw_line_segment_mut, draw_text_mut, text_size};
use log::{debug, error, info};
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
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

    /// AOOSTAR-X json configuration file to parse.
    ///
    /// The configuration file will be loaded from the `config_dir` directory if no full path is
    /// specified.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Configuration directory containing monnfiguration files and background images
    /// specified in the `config` file. Default: `./cfg`
    #[arg(long)]
    config_dir: Option<PathBuf>,

    /// Font directory for fonts specified in the `config` file. Default: `./fonts`
    #[arg(long)]
    font_dir: Option<PathBuf>,

    /// Single sensor value input file or directory for multiple sensor input files.
    /// Default: `./cfg/sensors`
    #[arg(long)]
    sensor_path: Option<PathBuf>,

    /// Run a demo
    #[arg(long)]
    demo: bool,

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

    if !args.demo
        && let Some(config) = args.config
    {
        info!("Starting sensor panel mode");
        let img_save_path = if args.save {
            let img_save_path = PathBuf::from("out");
            fs::create_dir_all(&img_save_path)?;
            Some(img_save_path)
        } else {
            None
        };

        let cfg_dir = args.config_dir.unwrap_or_else(|| "cfg".into());
        let cfg = load_configuration(&config, &cfg_dir)?;
        run_sensor_panel(
            &mut screen,
            cfg,
            cfg_dir,
            args.font_dir.unwrap_or_else(|| "fonts".into()),
            args.sensor_path.unwrap_or_else(|| "cfg/sensors".into()),
            img_save_path,
        )?;
        return Ok(());
    }

    if let Some(image) = args.image {
        info!("Loading and displaying background image {image}...");
        let rgb_img = img::load_image(&image, DISPLAY_SIZE)?;
        let timestamp = Instant::now();
        screen.send_image(&rgb_img)?;
        debug!("Image sent in {}ms", timestamp.elapsed().as_millis());
    }

    if args.demo {
        info!("Loading and displaying demo...");
        run_demo(
            &mut screen,
            args.config.as_deref(),
            args.config_dir.unwrap_or_else(|| "cfg".into()),
            args.font_dir.unwrap_or_else(|| "fonts".into()),
            args.save,
        )?;
    }

    if let Some(off) = args.off_after {
        info!("Switching off display in {off}s");
        sleep(Duration::from_secs(off as u64));
        screen.off()?;
    }

    info!("Bye bye!");

    Ok(())
}

fn load_configuration<P: AsRef<Path>>(config: P, config_dir: P) -> anyhow::Result<MonitorConfig> {
    let config = config.as_ref();
    let config_dir = config_dir.as_ref();

    if config.is_absolute() {
        cfg::load_cfg(config)
    } else {
        cfg::load_cfg(config_dir.join(config))
    }
}

fn run_sensor_panel<P: AsRef<Path>, B: Into<PathBuf>>(
    screen: &mut AooScreen,
    mut cfg: MonitorConfig,
    config_dir: B,
    font_dir: B,
    sensor_path: B,
    img_save_path: Option<P>,
) -> anyhow::Result<()> {
    let config_dir = config_dir.into();
    let sensor_values: Arc<RwLock<HashMap<String, String>>> = Arc::new(RwLock::new(HashMap::new()));
    let mut fh = FontHandler::new(font_dir);

    let mut rgb_img;
    let mut save_img_name;

    start_file_slurper(sensor_path, sensor_values.clone())?;

    let refresh = Duration::from_millis((cfg.setup.refresh * 1000f32) as u64);

    let switch_time = cfg
        .setup
        .switch_time
        .as_deref()
        .and_then(|v| f32::from_str(v).ok())
        .map(|v| Duration::from_millis((v * 1000.0) as u64))
        .unwrap_or(Duration::from_secs(30));

    // panel switching loop
    loop {
        let panel = cfg
            .get_next_active_panel()
            .ok_or(anyhow!("No active panel"))?;

        if let Some(img_file) = &panel.img {
            let img_file = PathBuf::from(img_file);
            save_img_name = img_file
                .file_stem()
                .map(|s| s.to_string_lossy().to_string());
            let file = if img_file.is_absolute() {
                img_file
            } else {
                config_dir.join(img_file)
            };
            info!("Loading panel image {file:?}...");
            rgb_img = img::load_image(&file, DISPLAY_SIZE)?;
        } else {
            save_img_name = None;
            rgb_img = RgbImage::new(DISPLAY_SIZE.0, DISPLAY_SIZE.1);
        }

        let panel_switch_time = Instant::now();

        // active panel refresh loop
        let mut refresh_count = 1;
        loop {
            let upd_start_time = Instant::now();

            let out_filename = if let Some(save_path) = &img_save_path {
                let save_path = save_path.as_ref();
                Some(save_path.join(format!(
                    "{}-{refresh_count:02}.png",
                    save_img_name.as_deref().unwrap_or("panel")
                )))
            } else {
                None
            };

            update_panel(
                screen,
                &rgb_img,
                &mut fh,
                panel,
                sensor_values.clone(),
                out_filename,
            )?;

            let elapsed = upd_start_time.elapsed();
            if refresh > elapsed {
                sleep(refresh - elapsed);
            }

            if panel_switch_time.elapsed() >= switch_time {
                info!("Switching panels");
                break;
            }

            refresh_count += 1;
        }
    }
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
        let mut cfg = load_configuration(config, &config_dir)?;

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

            let mut fh = FontHandler::new(font_dir);
            let out_filename = if save_images {
                fs::create_dir_all("out")?;
                Some("out/demo_panel.png")
            } else {
                None
            };

            update_panel(
                screen,
                &rgb_img,
                &mut fh,
                panel,
                Arc::new(RwLock::new(demo_values)),
                out_filename,
            )?;
        } else {
            error!("No active panel found");
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

fn update_panel<P: AsRef<Path>>(
    screen: &mut AooScreen,
    background: &RgbImage,
    fh: &mut FontHandler,
    panel: &Panel,
    values: Arc<RwLock<HashMap<String, String>>>,
    img_save_path: Option<P>,
) -> anyhow::Result<()> {
    debug!(
        "Displaying panel {}...",
        panel
            .name
            .as_deref()
            .unwrap_or_else(|| panel.id.as_deref().unwrap_or_default())
    );

    let mut rgb_img = background.clone();

    for sensor in &panel.sensor {
        if sensor.mode != SensorMode::Text {
            debug!(
                "Skipping sensor {}: unsupported sensor mode {:?}",
                sensor.label, sensor.mode
            );
            continue;
        }

        let values = values.read().expect("RwLock is poisoned");
        let value = values.get(&sensor.label).cloned();
        let unit = values
            .get(&format!("{}#unit", sensor.label))
            .cloned()
            .or_else(|| sensor.unit.clone())
            .unwrap_or_default();
        drop(values);

        if let Some(value) = value {
            let font = fh.get_ttf_font_or_default(&sensor.font_family);
            // TODO verify pixel scaling! Is font_size point size or pixel size?
            // This is still a bit off compared to the original AOOSTAR-X. Only tested with HarmonyOS_Sans_SC_Bold!
            let adjustment_hack = 0.7;
            let scale = font
                .pt_to_px_scale(sensor.font_size as f32 * adjustment_hack)
                .unwrap();

            let text = format!("{value}{unit}");
            let size = text_size(scale, &font, &text);
            // TODO verify x & y-coordinate handling
            let x = match sensor.text_align {
                TextAlign::Left => sensor.x as i32,
                TextAlign::Center => sensor.x as i32 - (size.0 / 2) as i32,
                TextAlign::Right => sensor.x as i32 - size.0 as i32,
            };
            let y = (sensor.y - scale.y / 2f32) as i32;
            // let y = sensor.y as i32 - (size.1 / 2) as i32;

            debug!(
                "Sensor({:03},{:03}), pixel({x:03},{y:03}), size{size:?}: {text}",
                sensor.x, sensor.y
            );

            draw_text_mut(
                &mut rgb_img,
                sensor.font_color.into(),
                x,
                y,
                scale,
                &font,
                &text,
            );
        }
    }

    screen.send_image(&rgb_img)?;

    if let Some(path) = img_save_path {
        rgb_img.save_with_format(path, image::ImageFormat::Png)?;
    }

    Ok(())
}
