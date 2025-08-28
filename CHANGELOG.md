# AOOSTAR WTR MAX Screen Control Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

_Changes in the next release_

### Added
- Simple sensor panel with a file-based data source (#6) 
- Initial support for fan-, progress-, & pointer-sensors (#8)

### Changed
- Project structure using a Cargo workspace

---

## v0.1.0 - 2025-08-02
### Added
- Initial `asterctl` tool release for controlling the LCD: on, off, display an image
- systemd service file to switch off LCD on system start.
- Demo mode
