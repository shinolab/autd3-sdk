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
