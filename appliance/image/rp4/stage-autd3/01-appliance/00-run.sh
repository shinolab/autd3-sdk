#!/bin/bash -e

install -v -d "${ROOTFS_DIR}/usr/local/bin" \
  "${ROOTFS_DIR}/usr/local/sbin" \
  "${ROOTFS_DIR}/usr/local/libexec/autd3" \
  "${ROOTFS_DIR}/usr/lib/autd3/seed"

install -v -m 755 files/autd3-remote-server "${ROOTFS_DIR}/usr/local/bin/"
install -v -m 755 files/tune-appliance.sh "${ROOTFS_DIR}/usr/local/libexec/autd3/"
install -v -m 755 files/run-server "${ROOTFS_DIR}/usr/local/libexec/autd3/"
install -v -m 755 -o root -g root files/autd3-admin "${ROOTFS_DIR}/usr/local/sbin/"
install -v -m 440 -o root -g root files/sudoers-autd3-admin \
  "${ROOTFS_DIR}/etc/sudoers.d/autd3-admin"
install -v -m 644 files/autd3-remote-server.service "${ROOTFS_DIR}/etc/systemd/system/"

install -v -m 644 files/image-release "${ROOTFS_DIR}/usr/lib/autd3/image-release"

install -v -m 644 files/remote-server.toml "${ROOTFS_DIR}/usr/lib/autd3/seed/"

setcap cap_net_raw,cap_sys_nice,cap_ipc_lock+ep \
  "${ROOTFS_DIR}/usr/local/bin/autd3-remote-server" ||
  echo "could not set file capabilities; the unit grants them at runtime"

on_chroot << EOF
if ! getent passwd autd3 > /dev/null; then
    adduser --system --group --home /var/lib/autd3 --no-create-home \
            --shell /usr/sbin/nologin autd3
fi
usermod -aG systemd-journal autd3
systemctl enable autd3-remote-server.service
EOF
