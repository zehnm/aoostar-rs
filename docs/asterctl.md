# asterctl Documentation

> Aster: Greek for star and similar to AOOSTAR.

A work-in-progress "panel-mode" mimics the AOOSTAR-X software and uses the same configuration files for rendering sensor
panels with dynamic sensor values.

By default, the original LCD USB UART device `416:90A1` is used. See optional parameters to specify a different device.

```
./asterctl --help
AOOSTAR WTR MAX and GEM12+ PRO screen control

Usage: asterctl [OPTIONS]

Options:
  -d, --device <DEVICE>
          Serial device, for example "/dev/cu.usbserial-AB0KOHLS".
          Takes priority over --usb option

  -u, --usb <USB>
          USB serial UART "vid:pid" in hex notation (lsusb output). Default: 416:90A1

      --on
          Switch display on and exit. This will show the last displayed image

      --off
          Switch display off and exit

  -i, --image <IMAGE>
          Image to display, other sizes than 960x376 will be scaled

  -c, --config <CONFIG>
          AOOSTAR-X json configuration file to parse.
          
          The configuration file will be loaded from the `config_dir` directory
          if no full path is specified.

  -p, --panels <PANELS>
          Include one or more additional custom panels into the base configuration.
          
          Specify the path to the panel directory containing panel.json and fonts /
          img subdirectories.

      --config-dir <CONFIG_DIR>
          Configuration directory containing configuration files and background
          images specified in the `config` file. Default: `./cfg`

      --font-dir <FONT_DIR>
          Font directory for fonts specified in the `config` file. Default: `./fonts`

      --sensor-path <SENSOR_PATH>
          Single sensor value input file or directory for multiple sensor input files.
          Default: `./cfg/sensors`

  -o, --off-after <OFF_AFTER>
          Switch off display n seconds after loading image or running demo

  -w, --write-only
          Test mode: only write to the display without checking response

  -s, --save
          Test mode: save changed images in ./out folder

      --simulate
          Simulate serial port for testing and development,
          `--device` and `--usb` options are ignored

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## Sensor Panel Mode

```shell
asterctl --config monitor.json
```

## Control Commands

The following control commands are available to switch the display off or display a static image.

**Switch display on:**

```shell
asterctl --on
```
This will display the last image that was shown before the display was switched off.
This image is stored in the display firmware and not sent by `asterctl`.

**Switch display off:**

```shell
asterctl --off
```

Switching the display off is also possible with pure [shell commands](shell_commands.md).

**Display an image:**

```shell
asterctl --image img/aybabtu.png
```

This expects a 960 × 376 image (other sizes are automatically scaled and the aspect ratio is ignored).
See Rust image crate for [supported image formats](https://github.com/image-rs/image?tab=readme-ov-file#supported-image-formats).

## Demo app

```shell
cargo run --release --bin demo -- --config monitor.json
```

The `--config` parameter is optional. It loads the official configuration file and displays the defined sensors in the
first panel.

### Parameters

- `--device /dev/ttyACM0` — Specify the serial device.
- `--usb 0403:6001` — Specify the USB UART device by USB **VID:PID** (hexadecimal, as shown by `lsusb`).
- `--help` — Show all options.

