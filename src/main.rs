// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

mod cfg;
mod display;
mod dummy_serialport;
mod font;
mod format_value;
mod img;
mod render;
mod sensors;

use crate::cfg::{MonitorConfig, Panel, load_custom_panel};
use crate::display::{AooScreen, AooScreenBuilder, DISPLAY_SIZE};
use crate::font::FontHandler;
use crate::render::PanelRenderer;
use crate::sensors::start_file_slurper;
use ab_glyph::PxScale;
use anyhow::anyhow;
use clap::Parser;
use env_logger::Env;
use image::imageops::FilterType;
use image::{ImageReader, Rgb, RgbImage};
use imageproc::drawing::{draw_line_segment_mut, draw_text_mut};
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

    /// Include one or more additional custom panels into the base configuration.
    ///
    /// Specify the path to the panel directory containing panel.json and fonts / img subdirectories.
    #[arg(short, long)]
    panels: Option<Vec<PathBuf>>,

    /// Configuration directory containing configuration files and background images
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
        let cfg = load_configuration(&config, &cfg_dir, args.panels)?;
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
        let rgb_img = img::load_image(&image, Some(DISPLAY_SIZE))?.to_rgb8();
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

fn load_configuration<P: AsRef<Path>>(
    config: P,
    config_dir: P,
    panels: Option<Vec<PathBuf>>,
) -> anyhow::Result<MonitorConfig> {
    let config = config.as_ref();
    let config_dir = config_dir.as_ref();

    let mut cfg = if config.is_absolute() {
        cfg::load_cfg(config)?
    } else {
        cfg::load_cfg(config_dir.join(config))?
    };

    if let Some(panels) = panels {
        for panel in panels {
            cfg.include_custom_panel(load_custom_panel(panel)?);
        }
    }

    Ok(cfg)
}

fn run_sensor_panel<B: Into<PathBuf>>(
    screen: &mut AooScreen,
    mut cfg: MonitorConfig,
    config_dir: B,
    font_dir: B,
    sensor_path: B,
    img_save_path: Option<B>,
) -> anyhow::Result<()> {
    let font_dir = font_dir.into();
    let config_dir = config_dir.into();
    let img_save_path = img_save_path.map(|p| p.into());

    let mut renderer = PanelRenderer::new(DISPLAY_SIZE, &font_dir, &config_dir);
    if let Some(img_save_path) = &img_save_path {
        renderer.set_img_save_path(img_save_path);
        renderer.set_save_render_img(true);
        // renderer.set_save_processed_pic(true);
        // renderer.set_save_progress_layer(true);
    }

    let sensor_values: Arc<RwLock<HashMap<String, String>>> = Arc::new(RwLock::new(HashMap::new()));

    start_file_slurper(sensor_path, sensor_values.clone())?;

    let refresh = Duration::from_millis((cfg.setup.refresh * 1000f32) as u64);

    let switch_time = cfg
        .setup
        .switch_time
        .as_deref()
        .and_then(|v| f32::from_str(v).ok())
        .map(|v| Duration::from_millis((v * 1000.0) as u64))
        .unwrap_or(Duration::from_secs(5));

    // panel switching loop
    loop {
        let panel = cfg
            .get_next_active_panel()
            .ok_or(anyhow!("No active panel"))?;

        info!("Switching panel: {}", panel.friendly_name());
        let panel_switch_time = Instant::now();

        // active panel refresh loop
        let mut refresh_count = 1;
        loop {
            let upd_start_time = Instant::now();

            if img_save_path.is_some() {
                renderer.set_img_suffix(format!("-{refresh_count:02}"));
            }

            // Keeping the read lock during panel rendering should be ok, otherwise we could always clone the HashMap
            let values = sensor_values.read().expect("RwLock is poisoned");
            update_panel(screen, &mut renderer, panel, &values)?;
            drop(values);

            let elapsed = upd_start_time.elapsed();
            if refresh > elapsed {
                sleep(refresh - elapsed);
            }

            if panel_switch_time.elapsed() >= switch_time {
                break;
            }

            refresh_count += 1;
        }
    }
}

fn update_panel(
    screen: &mut AooScreen,
    renderer: &mut PanelRenderer,
    panel: &Panel,
    values: &HashMap<String, String>,
) -> anyhow::Result<()> {
    debug!("Displaying panel '{}'...", panel.friendly_name());

    match renderer.render(panel, values) {
        Ok(image) => screen.send_image(&image)?,
        Err(e) => error!("Error rendering panel '{}': {e:?}", panel.friendly_name()),
    }

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
        let mut cfg = load_configuration(config, &config_dir, None)?;

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

            update_panel(screen, &mut renderer, panel, &demo_values)?;
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
