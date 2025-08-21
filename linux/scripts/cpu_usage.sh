#!/bin/bash
# Simple proof-of-concept Linux shell script to periodically write the CPU usage into a sensor text file.
#
# A single sensor `cpu_percent` is written to the sensor file containing the overall cpu usage in percent.
# CPU usage is calculated every second.
#
# CPU usage logic from:
# https://stackoverflow.com/questions/9229333/how-to-get-overall-cpu-usage-e-g-57-on-linux
#
set -e
set -o pipefail

REFRESH=1
TEMP_DIR="${TMPDIR:-/tmp}"
TMP_SENSOR_FILE="${TEMP_DIR}/cpu.txt.tmp"
SENSOR_FILE="${TEMP_DIR}/sensors/cpu.txt"

#=============================================================

usage() {
  cat << EOF
Simple PoC script to periodically write the CPU usage into a sensor text file.

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
      TMP_SENSOR_FILE="$OPTARG/cpu.txt.tmp"
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
  USAGE=$({ head -n1 /proc/stat;sleep "$REFRESH";head -n1 /proc/stat; } | awk '/^cpu /{u=$2-u;s=$4-s;i=$5-i;w=$6-w}END{print "cpu_percent:"int(0.5+100*(u+s+w)/(u+s+i+w))}')
  echo "$USAGE" > "$TMP_SENSOR_FILE"
  mv "$TMP_SENSOR_FILE" "$SENSOR_FILE"
done
