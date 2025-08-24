# asterctl Documentation

- [Linux Shell Control Commands](shell_commands.md) without using asterctl

## Sensor Panels

- [Sensor panels](sensor_panels.md)
- [Custom sensor panels](sensor_custom_panel.md)

### Sensor Modes

Different sensor modes are supported:

- [Sensor mode 1: Text](sensor_mode1_text.md)
- [Sensor mode 2: Circular Progress](sensor_mode2_fan.md)
- [Sensor mode 3: Progress](sensor_mode3_progress.md)
- [Sensor mode 4: Pointer](sensor_mode4_pointer.md)

### Sensor Data Sources

The sensor value reading is separated from the `asterctl` tool.

Sensor values are provided in separate text files and are automatically read when the file changes.  
Only the file data source is supported at the moment, other sources like pipes, sockets etc. might be supported later.

- [Text file data source](sensor_data_txt_file.md)

### Sensor Data Providers

- Proof of concept [Linux shell scripts](sensor_data_shell.md)
- [sysinfo tool](sensor_data_sysinfo.md)

## Development

- [LCD Protocol](lcd_protocol.md)

