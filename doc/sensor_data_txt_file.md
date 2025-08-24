# Text File Data Source

- Text file with ending: `.txt`
- Simple key / value pairs, separated by a colon `:`. Example: `foo: bar`
- Line based: one key / value per line.
- Key and value are trimmed. Any whitespace will be removed.
- Empty lines and comments are ignored.
- Comments start with `#` at the beginning of the line.
- Support for special keys: if key ends with `#unit` then the value is the unit for the corresponding key before the suffix
    - Example: `net_download_speed#unit: M/S` is the unit value for `net_download_speed`.
    - This can be used for dynamic unit values if they sensor value provider cannot add the unit to the corresponding value.
- File contents will automatically be read when updated.
    - This requires the sensor value provider to use atomic file updates!
    - Best practice is to use a temporary file on the same filesystem and use a move or rename operation after all values have been written.
- One or multiple sensor text files are supported.
    - Either a single file can be specified, or a directory path.
    - If a directory is specified, all children matching the sensor file naming pattern will be read and monitored.
    - Any subdirectories are ignored (no recursive support).

Example text file for the [cfg/monitor.json](../cfg/monitor.json) panel configuration:

```
cpu_temperature: 65
cpu_percent: 98
memory_usage: 77
memory_Temperature: 48
net_ip_address: 146.56.182.244
gpu_core: 98
gpu_temperature: 78
net_upload_speed: 100
net_upload_speed#unit: K/S
net_download_speed: 120
net_download_speed#unit: M/S
motherboard_temperature: 38
storage_ssd[0]['temperature']: 31
storage_ssd[0]['used']: 17
storage_ssd[1]['temperature']: 32
storage_ssd[1]['used']: 27
storage_ssd[2]['temperature']: 33
storage_ssd[2]['used']: 37
storage_ssd[3]['temperature']: 34
storage_ssd[3]['used']: 47
storage_ssd[4]['temperature']: 35
storage_ssd[4]['used']: 57
storage_hdd[0]['temperature']: 36
storage_hdd[0]['used']: 17
storage_hdd[1]['temperature']: 37
storage_hdd[1]['used']: 27
storage_hdd[2]['temperature']: 38
storage_hdd[2]['used']: 37
storage_hdd[3]['temperature']: 39
storage_hdd[3]['used']: 47
storage_hdd[4]['temperature']: 40
storage_hdd[4]['used']: 57
storage_hdd[5]['temperature']: 10
storage_hdd[5]['used']: 67
```
