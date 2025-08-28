# AOOSTAR WTR MAX / GEM12+ PRO Screen Control

Reverse engineering the [AOOSTAR WTR MAX](https://aoostar.com/products/aoostar-wtr-max-amd-r7-pro-8845hs-11-bays-mini-pc)
display protocol, with a proof-of-concept application written in Rust.  
It has only been tested on the WTR MAX, but should also support the GEM12+ PRO device.

**Disclaimer: ‼️ EXPERIMENTAL — use at your own risk ‼️**

> I take no responsibility for the use of this software.  
> There is no official documentation available;
> all display control commands have been reverse engineered from the original AOOSTAR-X software.

Even though this software works fine **for me**, I cannot guarantee that it is risk-free:

- It may or may not work.
- It could crash the display firmware, requiring a power cycle.
- It could even brick the display firmware.
- You have been warned!

The risk remains until the manufacturer provides official documentation, and the protocol can be reviewed.
Note: Multiple attempts to contact the manufacturer for documentation have received no response.

With that out of the way, on to the fun stuff!

**See [Linux shell commands](docs/shell_commands.md) on how to switch off the display with standard Linux commands!**

See [releases](https://github.com/zehnm/aoostar-rs/releases) for binary Linux x64 releases and [Linux systemd Service](linux/)
on how to automatically switch off the LCD at boot up. A Debian package for easy installation is planned for the future!

## Reverse Engineering

Reverse engineered LCD commands: [docs/lcd_protocol.md](docs/lcd_protocol.md)

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
- [ ] Eventually, publish a Rust crate for easy integration into other Rust applications.

**Out of scope:**

- Reverse engineering the microcontroller firmware on the display board.  
  That would be an interesting task — potentially uncovering additional display commands — but is outside the project's current scope.
- Reimplementing the full AOOSTAR-X display software, which is overly complex for most use cases.

### Features

- Control the AOOSTAR WTR MAX and GEM12+ PRO second screen from Linux.
- Switch the display on or off.
- Display images (with automatic scaling and partial update support).
- Render dynamic sensor panels defined from the AOOSTAR-X software.
  - Update sensor values from simple text files.
  - Rotate through multiple panels in a defined interval.
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

A release build is highly recommended, as it significantly improves graphic rendering performance:

```shell
cargo build --release
```

### Install

See [Linux systemd Service](linux/) on how to automatically switch off the LCD at boot up.

## Usage

See [asterctl documentation](docs/README.md) for more information or run `asterctl --help` for available command line options.

## Contributing

Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

Please note that this software is currently in its initial development and will have major changes until the mentioned
goals above are reached!

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
