#!/bin/bash -e

install -v -d "${ROOTFS_DIR}/etc/initramfs-tools/scripts" \
  "${ROOTFS_DIR}/etc/initramfs-tools/hooks"

install -v -m 755 files/overlay "${ROOTFS_DIR}/etc/initramfs-tools/scripts/overlay"
install -v -m 755 files/autd3-overlay "${ROOTFS_DIR}/etc/initramfs-tools/hooks/autd3-overlay"

on_chroot << 'EOF'
set -e
kver=$(ls -1 /lib/modules | grep -- '-rpi-v8$' | sort -V | tail -1)
[ -n "$kver" ] || { echo "no Raspberry Pi 4 kernel (-rpi-v8) under /lib/modules" >&2; exit 1; }
mkinitramfs -o /boot/firmware/initrd.img "$kver"

lsinitramfs /boot/firmware/initrd.img > /tmp/initrd.list
grep -qx 'scripts/overlay' /tmp/initrd.list ||
    { echo "the overlay boot script is not in the initramfs" >&2; exit 1; }
grep -q '/overlay\.ko' /tmp/initrd.list ||
    { echo "the overlay module is not in the initramfs" >&2; exit 1; }
rm -f /tmp/initrd.list
echo "initramfs built for ${kver} with the overlay script and module"
EOF

grep -q '^initramfs initrd.img' "${ROOTFS_DIR}/boot/firmware/config.txt" || cat >> \
  "${ROOTFS_DIR}/boot/firmware/config.txt" << 'EOF'
initramfs initrd.img followkernel
EOF
