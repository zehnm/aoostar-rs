# AOOSTAR WTR MAX Screen Control

- [asterctl usage](asterctl.md)
- [Linux shell control commands](shell_commands.md) without using asterctl

## Sensor Panels

- [Sensor panels](sensor/panels.md)
- [Custom sensor panels](sensor/custom_panel.md)

### Sensor Modes

Different sensor modes are supported:

- [Sensor mode 1: Text](sensor/cfg/mode1_text.md)
- [Sensor mode 2: Circular Progress](sensor/cfg/mode2_circular.md)
- [Sensor mode 3: Progress](sensor/cfg/mode3_progress.md)
- [Sensor mode 4: Pointer](sensor/cfg/mode4_pointer.md)

### Sensor Data Sources

The sensor value reading is separated from the `asterctl` tool.

Sensor values are provided in separate text files and are automatically read when the file changes.  
Only the file data source is supported at the moment, other sources like pipes, sockets etc. might be supported later.

- [Text file data source](sensor/provider/text_file.md)

### Sensor Data Providers

- Proof of concept [Linux shell scripts](sensor/provider/shell_scripts.md)
- [sysinfo tool](sensor/provider/sysinfo.md)

## Development

- [LCD Protocol](lcd_protocol.md)

