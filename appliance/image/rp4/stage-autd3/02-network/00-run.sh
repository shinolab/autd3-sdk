#!/bin/bash -e

install -v -d "${ROOTFS_DIR}/etc/udev/rules.d" \
  "${ROOTFS_DIR}/etc/NetworkManager/conf.d" \
  "${ROOTFS_DIR}/usr/local/libexec/autd3" \
  "${ROOTFS_DIR}/usr/lib/autd3/seed/network" \
  "${ROOTFS_DIR}/var/lib/NetworkManager"

install -v -m 644 files/76-autd3-interfaces.rules "${ROOTFS_DIR}/etc/udev/rules.d/"
install -v -m 644 files/10-autd3-ecat.conf "${ROOTFS_DIR}/etc/NetworkManager/conf.d/"

install -v -m 600 -o root -g root files/NetworkManager.state \
  "${ROOTFS_DIR}/var/lib/NetworkManager/NetworkManager.state"

install -v -m 755 files/autd3-wifi-init "${ROOTFS_DIR}/usr/local/libexec/autd3/"
install -v -m 644 files/autd3-wifi-init.service "${ROOTFS_DIR}/etc/systemd/system/"

install -v -m 600 -o root -g root files/autd3-uplink.nmconnection \
  "${ROOTFS_DIR}/usr/lib/autd3/seed/network/"

on_chroot << EOF
set -e
apt-get -y purge avahi-daemon
apt-get -y autoremove --purge
systemctl enable autd3-wifi-init.service
EOF
