#!/bin/sh

reflash () {
  arm-none-eabi-gdb -nx --batch \
    -ex 'target extended-remote /dev/ttyBmpGdb' \
    -ex 'monitor swdp_scan' \
    -ex 'attach 1' \
    -ex 'load' \
    -ex 'compare-sections' \
    -ex 'kill' \
    target/thumbv6m-none-eabi/debug/ev2400-fuckup
}

run () {
  echo "waiting for the logs..."
  socat /dev/ttyBmpTarg,rawer,b115200 STDOUT | defmt-print -e target/thumbv6m-none-eabi/debug/ev2400-fuckup
}

for i in $@; do
  case $i in
  flash)
    command_handled=1
    reflash ;;
  logs)
    command_handled=1
    run ;;
  esac
done

[ -z "$command_handled" ] && echo "usage: $0 <flash | logs>"
