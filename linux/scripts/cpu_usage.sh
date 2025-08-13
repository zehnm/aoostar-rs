#!/bin/bash
#
# https://stackoverflow.com/questions/9229333/how-to-get-overall-cpu-usage-e-g-57-on-linux
#

while :
do
  USAGE=$({ head -n1 /proc/stat;sleep 1;head -n1 /proc/stat; } | awk '/^cpu /{u=$2-u;s=$4-s;i=$5-i;w=$6-w}END{print "cpu_percent:"int(0.5+100*(u+s+w)/(u+s+i+w))}')
  echo "$USAGE" > /tmp/cpu.txt.tmp
  mv /tmp/cpu.txt.tmp /tmp/sensors/cpu.txt
done