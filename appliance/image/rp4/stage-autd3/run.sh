#!/bin/bash -e

FIRMWARE="${ROOTFS_DIR}/boot/firmware"

assert_dropin_wins() {
  local ours="$1" name key other
  name=$(basename "${ours}")
  shift
  for dir in "$@"; do
    for other in "${ROOTFS_DIR}${dir}"/*.conf; do
      [ -e "${other}" ] || continue
      [ "$(basename "${other}")" = "${name}" ] && continue
      [ "$(printf '%s\n%s\n' "${name}" "$(basename "${other}")" | sort | tail -1)" = "${name}" ] &&
        continue
      for key in $(sed -n 's/^\([A-Za-z][A-Za-z0-9]*\)=.*/\1/p' "${ours}"); do
        if grep -q "^${key}=" "${other}"; then
          echo "ERROR: ${other} sorts after ${name} and also sets ${key};" \
            "systemd would use theirs and drop ours silently" >&2
          exit 1
        fi
      done
    done
  done
}

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

install -v -d "${ROOTFS_DIR}/etc/systemd/journald.conf.d" \
  "${ROOTFS_DIR}/etc/systemd/system.conf.d" \
  "${ROOTFS_DIR}/etc/systemd/system/autd3-remote-server.service.d" \
  "${ROOTFS_DIR}/usr/local/libexec/autd3" \
  "${ROOTFS_DIR}/data"

rm -fv "${ROOTFS_DIR}"/etc/systemd/journald.conf.d/*autd3*.conf \
  "${ROOTFS_DIR}"/etc/systemd/system.conf.d/*autd3*.conf \
  "${ROOTFS_DIR}"/etc/systemd/system/autd3-remote-server.service.d/*.conf

install -v -m 644 files/90-autd3-journald.conf "${ROOTFS_DIR}/etc/systemd/journald.conf.d/"
install -v -m 644 files/90-autd3-watchdog.conf "${ROOTFS_DIR}/etc/systemd/system.conf.d/"

assert_dropin_wins files/90-autd3-journald.conf \
  /usr/lib/systemd/journald.conf.d /etc/systemd/journald.conf.d
assert_dropin_wins files/90-autd3-watchdog.conf \
  /usr/lib/systemd/system.conf.d /etc/systemd/system.conf.d
install -v -m 644 files/10-autd3-image.conf \
  "${ROOTFS_DIR}/etc/systemd/system/autd3-remote-server.service.d/"
install -v -m 755 files/autd3-firstboot "${ROOTFS_DIR}/usr/local/libexec/autd3/"
install -v -m 644 files/autd3-firstboot.service "${ROOTFS_DIR}/etc/systemd/system/"

grep -q '^LABEL=autd3-data' "${ROOTFS_DIR}/etc/fstab" ||
  cat >> "${ROOTFS_DIR}/etc/fstab" << 'EOF'
LABEL=autd3-data  /data  ext4  defaults,noatime,nodev,nofail,x-systemd.device-timeout=10  0  2
EOF

rm -rf "${ROOTFS_DIR}/etc/autd3" "${ROOTFS_DIR}/etc/NetworkManager/system-connections"
ln -sv /data/autd3 "${ROOTFS_DIR}/etc/autd3"
ln -sv /data/network "${ROOTFS_DIR}/etc/NetworkManager/system-connections"

sed -i 's/\bresize\b//' "${FIRMWARE}/cmdline.txt"
sed -i 's/ quiet//; s/ splash//; s/ plymouth\.ignore-serial-consoles//' "${FIRMWARE}/cmdline.txt"
CMDLINE_APPEND="$(cat files/cmdline-append.txt)"
grep -qF -- "${CMDLINE_APPEND}" "${FIRMWARE}/cmdline.txt" ||
  sed -i "1 s#\$# ${CMDLINE_APPEND}#" "${FIRMWARE}/cmdline.txt"

grep -q '^dtoverlay=disable-bt' "${FIRMWARE}/config.txt" || cat >> "${FIRMWARE}/config.txt" << 'EOF'

dtoverlay=disable-bt
dtparam=audio=off
disable_splash=1
EOF

on_chroot << EOF
set -e
systemctl disable rpi-resize.service || echo "could not disable rpi-resize (already gone?)"

apt-get -y purge rpi-swap apt-listchanges
systemctl disable hciuart.service 2> /dev/null || true
systemctl disable bluetooth.service 2> /dev/null || true

systemctl unmask systemd-timesyncd.service 2> /dev/null || true
systemctl enable systemd-timesyncd.service

systemctl enable autd3-firstboot.service
EOF

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

if [ "${AUTD3_LOCK_ACCOUNT}" = "1" ]; then
  on_chroot << EOF
passwd -l "${FIRST_USER_NAME}"
EOF
  if [ -n "${PUBKEY_SSH_FIRST_USER}" ]; then
    echo "the ${FIRST_USER_NAME} password is locked; the build's SSH key is the way in"
  else
    echo "the ${FIRST_USER_NAME} password is locked and no SSH key was given: no shell access"
  fi
fi
