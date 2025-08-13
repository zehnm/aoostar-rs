#!/bin/bash

while :
do
  MEM_TOTAL=$(grep MemTotal /proc/meminfo | awk '{print $2}')
  MEM_FREE=$(grep MemFree /proc/meminfo | awk '{print $2}')
  MEM_AVAILABLE=$(grep MemAvailable /proc/meminfo | awk '{print $2}')

  printf "memory_total:%d\nmemory_free:%d\nmemory_available:%d\nmemory_usage:%d\n" \
    "$MEM_TOTAL" \
    "$MEM_FREE" \
    "$MEM_AVAILABLE" \
    $(((MEM_TOTAL - MEM_AVAILABLE) * 100 / MEM_TOTAL)) > /tmp/mem.txt.tmp
  mv /tmp/mem.txt.tmp /tmp/sensors/mem.txt

  sleep 5
done
