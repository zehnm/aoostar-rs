// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

#![forbid(non_ascii_idents)]
#![deny(unsafe_code)]

use asterctl::burnin::{BurnInGuard, BurnInOptions, OffHours, invert};
use asterctl::cfg::{MonitorConfig, Panel, load_custom_panel};
use asterctl::render::PanelRenderer;
use asterctl::sensors::{
    STALE_VALUE, SensorStore, read_filter_file, read_key_value_file, start_file_slurper,
};
use asterctl::{cfg, img};
use asterctl_lcd::{AooScreen, AooScreenBuilder, DISPLAY_SIZE};

use anyhow::anyhow;
use clap::Parser;
use env_logger::Env;
use log::{debug, error, info};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

/// AOOSTAR WTR MAX and GEM12+ PRO screen control.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Serial device, for example, "/dev/cu.usbserial-AB0KOHLS". Takes priority over --usb option.
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
    /// specified in the `config` file.
    #[arg(long, default_value_t = String::from("cfg"))]
    config_dir: String, // default_value_t requires Display trait which PathBuf does not implement

    /// Font directory for fonts specified in the `config` file.
    #[arg(long, default_value_t = String::from("fonts"))]
    font_dir: String,

    /// Single sensor value input file or directory for multiple sensor input files.
    ///
    /// Can be specified multiple times. Paths that don't exist yet are picked up once they appear.
    /// Default: `cfg/sensors`
    #[arg(long)]
    sensor_path: Vec<PathBuf>,

    /// Show sensor values as `--` if their sensor file hasn't been updated for n seconds.
    ///
    /// 0 disables stale value detection.
    #[arg(long, default_value_t = 30)]
    stale_after: u32,

    /// Sensor identifier mapping file. Ignored if the file does not exist.
    ///
    /// The configuration file will be loaded from the `config_dir` directory if no full path is
    /// specified.
    #[arg(long, default_value_t = String::from("sensor-mapping.cfg"))]
    sensor_mapping: String,

    /// Burn-in protection: move the panel by one pixel every n minutes. 0 disables pixel shifting.
    #[arg(long, default_value_t = 0, value_name = "MINUTES")]
    pixel_shift: u32,

    /// Maximum pixel shift offset in each direction.
    #[arg(long, default_value_t = 2, value_name = "PIXELS")]
    pixel_shift_max: u32,

    /// Burn-in protection: show the inverted panel every n minutes. 0 disables wiping.
    #[arg(long, default_value_t = 0, value_name = "MINUTES")]
    wipe_interval: u32,

    /// Duration of the inverted panel.
    #[arg(long, default_value_t = 3, value_name = "SECONDS")]
    wipe_duration: u32,

    /// Switch off the display during this daily time window, for example `23:00-07:00`.
    #[arg(long, value_name = "HH:MM-HH:MM")]
    off_hours: Option<OffHours>,

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

    if let Some(config) = args.config {
        info!("Starting sensor panel mode");
        let img_save_path = if args.save {
            let img_save_path = PathBuf::from("out");
            fs::create_dir_all(&img_save_path)?;
            Some(img_save_path)
        } else {
            None
        };

        let cfg_dir = PathBuf::from(args.config_dir);
        let font_dir = PathBuf::from(args.font_dir);
        let sensor_paths = if args.sensor_path.is_empty() {
            vec![PathBuf::from("cfg/sensors")]
        } else {
            args.sensor_path
        };
        let stale_after =
            (args.stale_after > 0).then(|| Duration::from_secs(args.stale_after.into()));
        let minutes = |m: u32| (m > 0).then(|| Duration::from_secs(u64::from(m) * 60));
        let burn_in = BurnInOptions {
            pixel_shift_interval: minutes(args.pixel_shift),
            pixel_shift_max: args.pixel_shift_max,
            wipe_interval: minutes(args.wipe_interval),
            wipe_duration: Duration::from_secs(args.wipe_duration.into()),
            off_hours: args.off_hours,
        };
        let mapping_cfg = PathBuf::from(args.sensor_mapping);
        let cfg = load_configuration(&config, &cfg_dir, args.panels, &mapping_cfg)?;
        run_sensor_panel(
            &mut screen,
            cfg,
            cfg_dir,
            font_dir,
            PanelOptions {
                sensor_paths,
                stale_after,
                burn_in,
            },
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
    sensor_mapping: P,
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

    let sensor_mapping = sensor_mapping.as_ref();
    let mapping_cfg = if sensor_mapping.is_absolute() {
        sensor_mapping.to_path_buf()
    } else {
        config_dir.join(sensor_mapping)
    };
    if mapping_cfg.is_file() {
        let mut mapping = HashMap::new();
        read_key_value_file(&mapping_cfg, &mut mapping, None)?;
        cfg.set_sensor_mapping(mapping);
    } else {
        info!("Sensor mapping file {mapping_cfg:?} not found");
    }

    cfg.sensor_filter = load_sensor_filter(&mapping_cfg)?;

    Ok(cfg)
}

fn load_sensor_filter(mapping_cfg: &Path) -> anyhow::Result<Option<Vec<Regex>>> {
    if let Some(parent) = mapping_cfg.parent()
        && let Some(file_stem) = mapping_cfg.file_stem()
        && let Some(extension) = mapping_cfg.extension()
    {
        let filter_file = parent
            .join(format!("{}-filter", file_stem.to_string_lossy()))
            .with_extension(extension);

        if filter_file.is_file() {
            info!("Loading sensor filter file {filter_file:?}");
            return read_filter_file(filter_file);
        } else {
            info!("No sensor filter file {filter_file:?} available");
        }
    }

    Ok(None)
}

/// Sensor panel runtime options.
struct PanelOptions {
    sensor_paths: Vec<PathBuf>,
    stale_after: Option<Duration>,
    burn_in: BurnInOptions,
}

fn run_sensor_panel<B: Into<PathBuf>>(
    screen: &mut AooScreen,
    mut cfg: MonitorConfig,
    config_dir: B,
    font_dir: B,
    options: PanelOptions,
    img_save_path: Option<B>,
) -> anyhow::Result<()> {
    let PanelOptions {
        sensor_paths,
        stale_after,
        burn_in,
    } = options;
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

    let sensor_store = Arc::new(RwLock::new(SensorStore::new()));

    start_file_slurper(
        sensor_paths,
        sensor_store.clone(),
        cfg.sensor_filter.clone(),
    )?;

    let refresh = Duration::from_millis((cfg.setup.refresh * 1000f32) as u64);

    let switch_time = cfg
        .setup
        .switch_time
        .as_deref()
        .and_then(|v| f32::from_str(v).ok())
        .map(|v| Duration::from_millis((v * 1000.0) as u64))
        .unwrap_or(Duration::from_secs(5));

    let mut guard = BurnInGuard::new(burn_in);
    let mut display_off = false;

    let panel_count = cfg.active_panel_count();
    if panel_count == 0 {
        return Err(anyhow!("No active panel"));
    }

    // panel switching loop
    loop {
        // skip panels whose required sensor has no value. Show the next one if none qualifies.
        let values = snapshot(&sensor_store, stale_after);
        let mut skipped = 0;
        let panel = loop {
            let panel = cfg
                .get_next_active_panel()
                .ok_or(anyhow!("No active panel"))?;
            let available = panel
                .requires
                .as_ref()
                .is_none_or(|label| values.get(label).is_some_and(|v| v != STALE_VALUE));
            skipped += 1;
            if available || skipped >= panel_count {
                break panel;
            }
            debug!(
                "Skipping panel {}: no value for required sensor",
                panel.friendly_name()
            );
        };

        info!("Switching panel: {}", panel.friendly_name());
        let panel_switch_time = Instant::now();

        // active panel refresh loop
        let mut refresh_count = 1;
        loop {
            let upd_start_time = Instant::now();

            if img_save_path.is_some() {
                renderer.set_img_suffix(format!("-{refresh_count:02}"));
            }

            if guard.is_off_hours(chrono::Local::now().time()) {
                if !display_off {
                    info!("Off hours: switching display off");
                    screen.off()?;
                    display_off = true;
                }
                sleep(Duration::from_secs(10));
                continue;
            } else if display_off {
                info!("Off hours ended: switching display on");
                screen.on()?;
                // the display shows its last image: force sending a full frame
                screen.clear_cache();
                display_off = false;
            }

            let values = snapshot(&sensor_store, stale_after);
            update_panel(screen, &mut renderer, &mut guard, panel, &values)?;

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

fn snapshot(store: &RwLock<SensorStore>, stale_after: Option<Duration>) -> HashMap<String, String> {
    store
        .read()
        .expect("RwLock is poisoned")
        .snapshot(stale_after, SystemTime::now())
}

fn update_panel(
    screen: &mut AooScreen,
    renderer: &mut PanelRenderer,
    guard: &mut BurnInGuard,
    panel: &Panel,
    values: &HashMap<String, String>,
) -> anyhow::Result<()> {
    debug!("Displaying panel '{}'...", panel.friendly_name());

    let image = match renderer.render(panel, values) {
        Ok(image) => image,
        Err(e) => {
            error!("Error rendering panel '{}': {e:?}", panel.friendly_name());
            return Ok(());
        }
    };

    let now = Instant::now();
    let image = guard.apply(image, now);
    if guard.wipe_due(now) {
        debug!("Burn-in protection: showing inverted frame");
        screen.send_image(&invert(&image))?;
        sleep(guard.wipe_duration());
    }
    screen.send_image(&image)?;

    Ok(())
}
