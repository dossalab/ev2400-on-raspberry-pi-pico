#!/bin/sh

set -e

usage () {
  echo "usage: $0 {flash | logs} binary"
  exit 1
}

[ "$#" -ne 2 ] && usage

COMMAND=$1
EXECUTABLE=$2
THISDIR=$(dirname "$0")

flash () {
  arm-none-eabi-gdb -nx --batch -x "$THISDIR/blackmagic-flash.gdb" "$1"
}

run () {
  echo "waiting for the logs..."
  socat /dev/ttyBmpTarg,rawer,b115200 STDOUT | defmt-print -e "$1"
}

case $COMMAND in
flash)
  flash "$EXECUTABLE" ;;
logs)
  run "$EXECUTABLE" ;;
run)
  flash "$EXECUTABLE"
  run "$EXECUTABLE"
  ;;
*)
  usage ;;
esac
