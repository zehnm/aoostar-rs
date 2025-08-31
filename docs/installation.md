# Installation

There are multiple ways to install the `asterctl` CLI tool. Choose any one of the methods below that best suit your needs.

Please note that only Linux has been tested so far.

## Pre-compiled binaries

Executable binaries are available for download on the [GitHub Releases page](https://github.com/zehnm/aoostar-rs/releases).
Download the binary for your platform (only Linux available at the moment) and extract the archive.
The archive contains the `asterctl` and `aster-sysinfo` executables which you can run.

## Build from source using Rust

To build the `asterctl` and `aster-sysinfo` executables from source, you will first need to install Rust and Cargo.
Follow the instructions on the [Rust installation page](https://www.rust-lang.org/tools/install).
At least Rust version 1.88 is required.

The project contains IDE settings for [RustRover](https://www.jetbrains.com/rust/) (or other JetBrain IDEs with the Rust
plugin) to get you up and running in no time. This is not a requirement, everything can be easily built on the command line.

Once you have installed Rust, the following commands can be used to build `asterclt` and all other binaries:

1. On Linux, install required build dependencies (shown for Ubuntu 25.04):

```shell
sudo apt install build-essential git pkg-config libudev-dev
```

2. Checkout project:

```shell
git clone https://github.com/zehnm/aoostar-rs.git
cd aoostar-rs
```

3. Build

A release build is highly recommended, as it significantly improves graphic rendering performance:

```shell
cargo build --release
```

The binaries will be located in the `./target/release` folder.

>  A Debian package for easy installation is planned for the future!

See [Linux systemd Service](linux/) on how to automatically switch off the LCD at boot up.

## Contributing

Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.
