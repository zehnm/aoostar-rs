# System Sensor Provider for asterctl

This tool gathers system sensor values with the help of the [sysinfo](https://github.com/GuillaumeGomez/sysinfo) crate
and writes them into a text file.

See [README](../../README.md) in root directory for more information.

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

Single test run with printing all sensors in the console:
```shell
sysinfo --console
```

Normal mode providing sensor values for `asterctl` in `/tmp/sensors/sysinfo.txt` every 3 seconds:

```shell
sysinfo --refresh 3 --out /tmp/sensors/sysinfo.txt
```
