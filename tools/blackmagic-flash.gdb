target extended-remote /dev/ttyBmpGdb
monitor swdp_scan
attach 1
load
compare-sections
