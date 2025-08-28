# Sensor Configuration

Specify configuration file to use:
```shell
asterctl --config monitor.json
```

- The configuration file is loaded from the configuration directory if not an absolute path is specified.
- The default configuration directory is `./cfg` and can be changed with the `--config-dir` command line option.

The original AOOSTAR-X json configuration file format is used, but only a subset of the configuration is supported:

- Setup object fields:
    - `switchTime`: Optional switch time between panels in seconds, string value interpreted as float and converted to milliseconds. Default: 5
    - `refresh`: Panel redraw interval in seconds specified as a float number. Default: 1
- Panel object fields in `diy[]`:
    - `img`: Background image filename. Loaded from the specified configuration directory if not an absolute path is specified.
    - `sensor`: Array of sensor objects.
- Sensor object fields:
    - `label`: label identifier, also used as sensor value data source identifier
    - `integerDigits`: sensor value format option: number of integer places. Value is 0-prefixed to number of places and set to `99` if overflown.
    - `decimalDigits`: sensor value format option: number of decimal places.
    - `unit`: optional unit label, appended after the sensor value
    - `x`: x-position
    - `y`: y-position
    - `fontFamily`: Font name matching font filename without file extension. Fonts are loaded from the configured font directory.
    - `fontSize`: Font size
    - `fontColor`: Font color in `#RRGGBB` notation, or `-1` if not set. Examples: `#ffffff` = white, `#ff0000` = red. Default: `#ffffff`
    - `textAlign`: Text alignment: `left`, `right`, `center`
    - Fields used for the fan (2), progress (3) and pointer (4) sensor modes:
        - `min_value` and `max_value`
        - `width` and `height`
        - `direction`
        - `pic`: progress image, loaded from the specified configuration directory if not an absolute path is specified.
        - `min_angle` and `max_angle`
        - `xz_x` and `xz_y`

Example configuration file: [cfg/monitor.json](https://github.com/zehnm/aoostar-rs/blob/main/cfg/monitor.json).

Sensor values are not read from the configuration file (the `sensor.value` field is ignored).
See [Sensor Value Provider](../provider).

More options might be supported later.
