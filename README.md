# AOOSTAR WTR MAX Screen Control

Reverse engineering the [AOOSTAR WTR MAX](https://aoostar.com/products/aoostar-wtr-max-amd-r7-pro-8845hs-11-bays-mini-pc)
display protocol, with a proof-of-concept application written in Rust.  
This project should also support the GEM12+ PRO device.

**Disclaimer: ‼️ EXPERIMENTAL — use at your own risk ‼️**

> I take no responsibility for the use of this software.  
> There is no official documentation available;
> all display control commands have been reverse engineered from the original AOOSTAR-X software.

- It may or may not work.
- It could crash the display firmware, requiring a power cycle.
- It could even brick the display firmware.
- You have been warned!

The risk remains until the manufacturer provides official documentation, and the protocol can be reviewed.
Note: Multiple attempts to contact the manufacturer for documentation have received no response.

With that out of the way, on to the fun stuff!

**See [Linux shell commands](doc/shell_commands.md) on how to switch off the display with standard Linux commands!**

See [releases](https://github.com/zehnm/aoostar-rs/releases) for binary Linux x64 releases and [Linux systemd Service](linux/)
on how to automatically switch off the LCD at boot up. A Debian package for easy installation is planned for the future!

## Reverse Engineering

Reverse engineered LCD commands: [doc/lcd_protocol.md](doc/lcd_protocol.md)

### Motivation

Developing open client software to use the embedded second screen on various Linux distributions.
It *might* also work on Windows, but I neither have that OS, nor plan to install it.

The official proprietary AOOSTAR-X display software is not suitable for NAS and security-minded users:

- All-in-one solution that attempts to do everything, from sensor reading to running a web server for control and configuration (*exposed on all interfaces!*).  
  I prefer using existing monitoring tools and combining them to my liking.
- Resource hungry, written in Python. Archive of v1.3.4 is 178 MB.
- Closed source, requires root access, distributed over filesharing sites, some without HTTPS.
- Built-in expiration date. One must regularly update the software without being able to verify the source.
- Many untranslated messages in Chinese and missing instructions for included features.

The display remains on continuously (24×7) if the official software is not running.

### Goals

- [x] Reverse engineer the LCD serial protocol to provide open screen software.
    - Utilize the official AOOSTAR-X display software by sniffing USB communication, using `strace`, and decompiling the Python app.
- [x] Document known commands so clients in other programming languages can be written.
- [ ] Eventually, create a Rust crate for easy integration into other Rust applications.

**Out of scope:**

- Reverse engineering the microcontroller firmware on the display board.  
  That would be an interesting task — potentially uncovering additional display commands — but is outside the project's current scope.
- Reimplementing the full AOOSTAR-X display software, which is overly complex for most use cases.

### Features

- Control the AOOSTAR WTR MAX and GEM12+ PRO second screen from Linux.
- Switch the display on or off.
- Display images (with automatic scaling and partial update support).
- Proof-of-concept demo for drawing shapes and text.
- USB device/serial port selection.

## Setup

### Requirements

1. A recent [Rust](https://rustup.rs/) toolchain is required, using `rust-up` is the easiest way to get everything set up.

2. Install required build dependencies (shown for Ubuntu 25.04):

```shell
sudo apt install build-essential git pkg-config libudev-dev
```

3. Checkout project:

```shell
git clone https://github.com/zehnm/aoostar-rs.git
cd aoostar-rs
```

### Build

A release build is highly recommended, as it significantly improves graphics performance:

```shell
cargo build --release --bins --all-features
```

The `--bins` option builds the main `asterctl` app and all other tools.

### Install

See [Linux systemd Service](linux/) on how to automatically switch off the LCD at boot up.

## Demo App Usage

Currently, the project includes a proof-of-concept demo application that loads an image, draws rectangles, and writes
text over the image.

By default, the original LCD USB UART device `416:90A1` is used. See optional parameters to specify a different device.

```shell
cargo run --release -- --demo --config monitor.json
```

The `--config` parameter is optional. It loads the official configuration file and displays the defined sensors in the
first panel.

### Parameters

- `--device /dev/ttyACM0` — Specify the serial device.
- `--usb 0403:6001` — Specify the USB UART device by USB **VID:PID** (hexadecimal, as shown by `lsusb`).
- `--help` — Show all options.


### Sensor Panel

```shell
asterctl --config monitor.json
```

See [sensor panels](doc/sensor_panels.md) for more information.

### Control Commands

> Aster: Greek for star and similar to AOOSTAR.

Besides demo mode, the following control commands have been implemented.

The `asterctl` binary is built in `./target/release`.  
Alternatively, use `cargo run --release --` to build and run automatically, for example: 

```shell
cargo run --release -- --off
```

**Switch display on:**

```shell
asterctl --on
```

**Switch display off:**

```shell
asterctl --off
```

**Load and display an image:**

```shell
asterctl --image img/aybabtu.png
```

This expects a 960 × 376 image (other sizes are automatically scaled and the aspect ratio is ignored).
See Rust image crate for [supported image formats](https://github.com/image-rs/image?tab=readme-ov-file#supported-image-formats).

## Contributing

Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

Please note that this software is currently in its initial development and will have major changes until the mentioned
goals above are reached!

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

