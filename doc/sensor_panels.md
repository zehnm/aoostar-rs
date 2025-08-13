# Sensor Panels

The `asterctl` tool is started in sensor panel mode if the `--config` command line option is specified.

Sensor panels are dynamic screens showing various sensor values. Multiple rotating panels are supported. 
The sensor values must be provided in simple key-value text files from external scripts or tools. The `asterctl` tool
is only responsible for rendering the panels on the embedded screen.

Example panels from the AOOSTAR-X software, rendered with `asterctl` using dummy sensor values:

<img src="img/sensor_panel-01.png" alt="Sensor panel 1">

<img src="img/sensor_panel-02.png" alt="Sensor panel 1">

## Supported Features

- One or multiple panels rotating in configurable interval (configuration value `setup.switchTime`).
- Each panel can be configured with multiple sensor fields.
  - Only text sensor value fields are supported (`sensor.mode: 1`).
  - Fan (2), progress (3) and pointer (4) sensors are not supported.
- Each sensor field can be customized with an individual font, size, color and text alignment.
- Panels are redrawn at a configurable interval (configuration value `setup.refresh`).
  - Only the updated areas of the image are sent to the display for faster updates.

## Panel Configuration File

Specify configuration file to use:
```shell
asterctl --config monitor.json
```

- The configuration file is loaded from the configuration directory if not an absolute path is specified.
- The default configuration directory is `./cfg` and can be changed with the `--config-dir` command line option.

The original AOOSTAR-X json configuration file format is used, but only use a subset of the configuration is supported:

- Setup object fields:
  - `switchTime`: Optional switch time between panels in seconds, string value interpreted as float and converted to milliseconds. Default: 5
  - `refresh`: Panel redraw interval in seconds specified as a float number. Default: 1
- Panel object fields in `diy[]`:
  - `img`: Background image filename. Loaded from the specified configuration directory if not an absolute path is specified.
  - `sensor`: Array of sensor objects.
- Sensor object fields:
  - `label`: label identifier, also used as sensor value data source identifier
  - `unit`: optional unit label, appended after the sensor value
  - `x`: x-position
  - `y`: y-position
  - `fontFamily`: Font name matching font filename without file extension. Fonts are loaded from the configured font directory.
  - `fontSize`: Font size
  - `fontColor`: Font color in `#RRGGBB` notation, or `-1` if not set. Examples: `#ffffff` = white, `#ff0000` = red. Default: `#ffffff` 
  - `textAlign`: Text alignment: `left`, `right`, `center`

Example configuration file: [cfg/monitor.json](../cfg/monitor.json).

Sensor values are not read from the configuration file (the `sensor.value` field is ignored). See data sources below.

More options might be supported later.

## Sensor Data Sources

Sensor values are provided in separate text files and are automatically read when the file changes.
Only the file data source is supported at the moment, other sources like pipes, sockets etc. might be supported later.

### Text File Data Source

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
