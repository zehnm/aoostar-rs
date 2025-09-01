# Sensor Panels

- [Sensor panels](panel.md)
- [Custom sensor panels](custom_panel.md)
- [Configuration](cfg/)

## Sensor Modes

Different sensor modes are supported:

- [Sensor mode 1: Text](cfg/mode1_text.md)
- [Sensor mode 2: Circular Progress](cfg/mode2_circular.md)
- [Sensor mode 3: Progress](cfg/mode3_progress.md)
- [Sensor mode 4: Pointer](cfg/mode4_pointer.md)

## Sensor Data Sources

The sensor value reading is separated from the `asterctl` tool.

Sensor values are provided in separate text files and are automatically read when the file changes.  
Only the file data source is supported at the moment, other sources like pipes, sockets etc. might be supported later.

- [Text file data source](provider/text_file.md)

### Sensor Data Providers

- Proof of concept [Linux shell scripts](provider/shell_scripts.md)
- [aster-sysinfo tool](provider/sysinfo.md)

### Sensor Identifier Mapping

The original AOOSTAR-X software uses very weird label identifiers (actually sometimes even a composite key depending on
the data source), which are likely based on an internal JSON structure.

To easily use original custom sensor panels with various sensor data sources, a sensor identifier mapping file can be used.

The mapping file is a simple text file with one identifier mapping per line:
- Key = label identifier used in panel definition
- Value = label identifier used in sensor providers

Example:

```
cpu_temperature: temperature_cpu
```

This maps the `temperature_cpu` sensor from the `aster-sysinfo` tool to the `cpu_temperature` sensor used in the
AOOSTAR-X panel definitions.

Usage example:
```shell
asterctl --config monitor.json --sensor-mapping sensor-mapping/sysinfo-to-aoostar.cfg
```
