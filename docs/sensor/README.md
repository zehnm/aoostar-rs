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
- [sysinfo tool](provider/sysinfo.md)
