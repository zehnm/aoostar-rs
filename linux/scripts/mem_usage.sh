#!/bin/bash
# Simple proof-of-concept Linux shell script to periodically write the memory usage into a sensor text file.
#
# The following sensor values are written to the sensor file.
# - memory_total in bytes
# - memory_free in bytes
# - memory_available in bytes
# - memory_usage in percent
#
# Memory usage is calculated every five seconds.

set -e
set -o pipefail

REFRESH=5
TEMP_DIR="${TMPDIR:-/tmp}"
TMP_SENSOR_FILE="${TEMP_DIR}/mem.txt.tmp"
SENSOR_FILE="${TEMP_DIR}/sensors/mem.txt"

#=============================================================

usage() {
  cat << EOF
Simple PoC script to periodically write the memory usage into a sensor text file.

Usage:
$0 [-r REFRESH] [-s SENSOR_FILE] [-t TEMP_DIR]

  -r REFRESH     refresh in seconds.  Default: $REFRESH
  -s SENSOR_FILE output sensor file.  Default: $SENSOR_FILE
  -t TEMP_DIR    temporary directory. Default: $TEMP_DIR

EOF
  exit 1
}

#=============================================================

#------------------------------------------------------------------------------
# Start of script
#------------------------------------------------------------------------------
# check command line arguments
while getopts "r:s:t:h" opt; do
  case ${opt} in
    "r")
      REFRESH="$OPTARG"
      ;;
    "s")
      SENSOR_FILE="$OPTARG"
      ;;
    "t")
      TMP_SENSOR_FILE="$OPTARG/mem.txt.tmp"
      ;;
    h )
        usage
        ;;
    : )
        echo "Option: -$OPTARG requires an argument" 1>&2
        usage
        ;;
   \? )
        echo "Invalid option: -$OPTARG" 1>&2
        usage
        ;;
  esac
done

mkdir -p "$(dirname "$SENSOR_FILE")"

while :
do
  MEM_TOTAL=$(grep MemTotal /proc/meminfo | awk '{print $2}')
  MEM_FREE=$(grep MemFree /proc/meminfo | awk '{print $2}')
  MEM_AVAILABLE=$(grep MemAvailable /proc/meminfo | awk '{print $2}')

  printf "memory_total:%d\nmemory_free:%d\nmemory_available:%d\nmemory_usage:%d\n" \
    "$MEM_TOTAL" \
    "$MEM_FREE" \
    "$MEM_AVAILABLE" \
    $(((MEM_TOTAL - MEM_AVAILABLE) * 100 / MEM_TOTAL)) > "$TMP_SENSOR_FILE"
  mv "$TMP_SENSOR_FILE" "$SENSOR_FILE"

  sleep "$REFRESH"
done
