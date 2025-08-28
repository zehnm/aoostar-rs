# Sensor Data Provider Shell Scripts

The [/linux/scripts](../../../linux/scripts) directory contains some proof-of-concept Linux shell scripts.

CPU and memory usage are written into a sensor data source text file that can be used by `asterctl`.

```
./cpu_usage.sh -h
Simple PoC script to periodically write the CPU usage into a sensor text file.

Usage:
./cpu_usage.sh [-r REFRESH] [-s SENSOR_FILE] [-t TEMP_DIR]

  -r REFRESH     refresh in seconds.  Default: 1
  -s SENSOR_FILE output sensor file.  Default: /tmp/sensors/cpu.txt
  -t TEMP_DIR    temporary directory. Default: /tmp
```

```
./mem_usage.sh -h
Simple PoC script to periodically write the memory usage into a sensor text file.

Usage:
./mem_usage.sh [-r REFRESH] [-s SENSOR_FILE] [-t TEMP_DIR]

  -r REFRESH     refresh in seconds.  Default: 5
  -s SENSOR_FILE output sensor file.  Default: /tmp/sensors/mem.txt
  -t TEMP_DIR    temporary directory. Default: /tmp
```
