# Introduction

- [asterctl usage](asterctl.md)
- [Linux shell control commands](shell_commands.md) without using asterctl

## Motivation

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

## Goals

- [x] Reverse engineer the LCD serial protocol to provide open screen software.
    - Utilize the official AOOSTAR-X display software by sniffing USB communication, using `strace`, and decompiling the Python app.
- [x] Document known commands so clients in other programming languages can be written.
- [ ] Eventually, publish a Rust crate for easy integration into other Rust applications.

**Out of scope:**

- Reverse engineering the microcontroller firmware on the display board.  
  That would be an interesting task — potentially uncovering additional display commands — but is outside the project's current scope.
- Reimplementing the full AOOSTAR-X display software, which is overly complex for most use cases.


## Development

- [Reverse engineered LCD protocol](lcd_protocol.md)
- [GitHub project](https://github.com/zehnm/aoostar-rs)

## Contributing

Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

## License

Licensed under either of

- [Apache License, Version 2.0](http://www.apache.org/licenses/LICENSE-2.0)
- [MIT License](http://opensource.org/licenses/MIT)

at your option.
