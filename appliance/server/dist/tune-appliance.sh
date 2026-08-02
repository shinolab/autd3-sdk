#!/bin/sh

set -u

IFACE="${AUTD3_ECAT_IFACE:-eth0}"
RT_CPU="${AUTD3_RT_CPU:-3}"

note() { echo "tune-appliance: $*"; }
try() { "$@" > /dev/null 2>&1 || note "skipped: $*"; }

try ip link set "$IFACE" up
for opt in "rx-usecs 0" "rx-frames 1" "tx-usecs 0" "tx-frames 1" \
  "adaptive-rx off" "adaptive-tx off"; do
  try ethtool -C "$IFACE" $opt
done
try ethtool --set-eee "$IFACE" eee off
for opt in "autoneg off" "rx off" "tx off"; do
  try ethtool -A "$IFACE" $opt
done
for opt in "gro off" "gso off" "tso off"; do
  try ethtool -K "$IFACE" $opt
done

OTHER_CPUS=$(awk -v rt="$RT_CPU" 'BEGIN {
    n = 0
    while ((getline line < "/proc/cpuinfo") > 0) if (line ~ /^processor/) n++
    sep = ""
    for (i = 0; i < n; i++) if (i != rt) { printf "%s%d", sep, i; sep = "," }
}')
irq_name() {
  ls "$1" 2> /dev/null | grep -vE \
    '^(smp_affinity|smp_affinity_list|affinity_hint|node|effective_affinity|effective_affinity_list|spurious)$' |
    head -1
}
STUCK=""
for irq in /proc/irq/[0-9]*; do
  [ -w "$irq/smp_affinity_list" ] || continue
  if ls "$irq" 2> /dev/null | grep -q "^${IFACE}\(\$\|-\)"; then
    echo "$RT_CPU" > "$irq/smp_affinity_list" 2> /dev/null ||
      note "skipped: pin irq ${irq##*/} to cpu $RT_CPU"
  elif [ -n "$OTHER_CPUS" ]; then
    echo "$OTHER_CPUS" > "$irq/smp_affinity_list" 2> /dev/null || {
      name=$(irq_name "$irq")
      case " $STUCK " in *" ${name:-?} "*) ;; *) STUCK="$STUCK ${name:-?}" ;; esac
    }
  fi
done
[ -n "$STUCK" ] && note "cannot be kept off cpu $RT_CPU (affinity is fixed):$STUCK"
[ -n "$OTHER_CPUS" ] && note "moved the non-EtherCAT interrupts to cpu $OTHER_CPUS"

for gov in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
  [ -w "$gov" ] && echo performance > "$gov" 2> /dev/null
done
[ -w /dev/cpu_dma_latency ] || note "skipped: /dev/cpu_dma_latency (use the kernel cmdline instead)"

note "done for $IFACE (rt cpu $RT_CPU)"
exit 0
