# sysinfo Tool

The Rust based [sysinfo](../crates/sysinfo/src/main.rs) tool gathers many more system sensor values with the help of
the [sysinfo](https://github.com/GuillaumeGomez/sysinfo) crate.

It supports FreeBSD, Linux, macOS, Windows and other OSes, but it has only been tested on Linux so far.

```
Proof of concept sensor value collection for the asterctl screen control tool

Usage: sysinfo [OPTIONS]

Options:
  -o, --out <OUT>
          Output sensor file

  -t, --temp-dir <TEMP_DIR>
          Temporary directory for preparing the output sensor file.
          
          The system temp directory is used if not specified.
          The temp directory must be on the same file system for atomic rename operation!

      --console
          Print values in console

  -r, --refresh <REFRESH>
          System sensor refresh interval in seconds

      --disk-refresh <DISK_REFRESH>
          Enable individual disk refresh logic as used in AOOSTAR-X. Refresh interval in seconds

      --smartctl
          Retrieve drive temperature if `disk-update` option is enabled.
          
          Requires smartctl and password-less sudo!
```

Single test run with printing all sensors on the console:
```shell
sysinfo --console
```

Normal mode providing sensor values for `asterctl` in `/tmp/sensors/sysinfo.txt`:

```shell
sysinfo --refresh 3 --out /tmp/sensor/sysinfo.txt
```

Note: the lower the refresh rate, the more resources are used!
