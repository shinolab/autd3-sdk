#!/bin/bash
set -euo pipefail
export LC_ALL=C

export PATH="${PATH}:/usr/local/sbin:/usr/sbin:/sbin"

CONFIG="$1"
IMAGE="$2"
STAGE_DIR="$3"
WORK_DIR="$4"
KERNEL="$5"
SLACK_MIB="$6"

ROOTFS_DIR="${WORK_DIR}/rootfs"
BOOT_DIR="${ROOTFS_DIR}/boot/firmware"
LOOP=""
export ROOTFS_DIR

. "${CONFIG}"

export TARGET_HOSTNAME LOCALE_DEFAULT KEYBOARD_KEYMAP TIMEZONE_DEFAULT
export FIRST_USER_NAME FIRST_USER_PASS AUTD3_LOCK_ACCOUNT
export PUBKEY_SSH_FIRST_USER="${PUBKEY_SSH_FIRST_USER:-}"

say() { echo "==> $*"; }

unmount_all() {
  local mount
  for mount in \
    "${ROOTFS_DIR}/dev/pts" "${ROOTFS_DIR}/dev" "${ROOTFS_DIR}/proc" "${ROOTFS_DIR}/sys" \
    "${ROOTFS_DIR}/run" "${ROOTFS_DIR}/tmp" "${BOOT_DIR}" "${ROOTFS_DIR}"; do
    if mountpoint -q "${mount}"; then umount -R "${mount}"; fi
  done
}

unwind() {
  set +e
  unmount_all
  [ -n "${LOOP}" ] && losetup -d "${LOOP}"
  return 0
}
trap unwind EXIT

attach() {
  LOOP=$(losetup --show --find --partscan "${IMAGE}" | cut -d ' ' -f 1)
  udevadm settle 2> /dev/null || true
  [ -b "${LOOP}p2" ] || {
    echo "the kernel did not expose ${LOOP}p2; is ${IMAGE} a partitioned image?" >&2
    exit 1
  }
}

detach() {
  losetup -d "${LOOP}"
  LOOP=""
}

fsck_rootfs() {
  e2fsck -p -f "${LOOP}p2" || [ $? -lt 4 ]
}

rootfs_start() {
  sfdisk -d "${IMAGE}" | sed -n 's/^.*2 *: *start= *\([0-9]*\).*$/\1/p'
}

fs_field() {
  dumpe2fs -h "${LOOP}p2" 2> /dev/null | sed -n "s/^$1: *//p"
}

grow_rootfs() {
  say "growing the rootfs into the space the image file gained"
  echo ', +' | sfdisk --no-reread --no-tell-kernel -N 2 "${IMAGE}" > /dev/null
  attach
  fsck_rootfs
  resize2fs "${LOOP}p2"
}

shrink_rootfs() {
  say "shrinking the rootfs, keeping at least ${SLACK_MIB} MiB free"
  fsck_rootfs
  resize2fs -M "${LOOP}p2"
  fsck_rootfs

  local block_size wanted short
  block_size=$(fs_field 'Block size')
  wanted=$((SLACK_MIB * 1024 * 1024 / block_size))
  short=$((wanted - $(fs_field 'Free blocks')))
  if [ "${short}" -gt 0 ]; then
    resize2fs "${LOOP}p2" "$(($(fs_field 'Block count') + short))"
    fsck_rootfs
  fi
}

finish_partition() {
  say "trimming the partition and the image file to the filesystem"
  local sectors start
  sectors=$(($(fs_field 'Block count') * $(fs_field 'Block size') / 512))
  start=$(rootfs_start)
  detach

  echo ", ${sectors}" | sfdisk --no-reread --no-tell-kernel -N 2 "${IMAGE}" > /dev/null
  truncate -s $(((start + sectors) * 512)) "${IMAGE}"
}

mount_filesystems() {
  install -d "${ROOTFS_DIR}"
  mount "${LOOP}p2" "${ROOTFS_DIR}"
  mount "${LOOP}p1" "${BOOT_DIR}"
}

mount_image() {
  say "mounting the image"
  mount_filesystems
  mount -t proc proc "${ROOTFS_DIR}/proc"
  mount -t sysfs sys "${ROOTFS_DIR}/sys"
  mount -t tmpfs tmpfs "${ROOTFS_DIR}/run"
  mount -t tmpfs tmpfs "${ROOTFS_DIR}/tmp"
  mount --bind /dev "${ROOTFS_DIR}/dev"
  mount --bind /dev/pts "${ROOTFS_DIR}/dev/pts"

  if [ -e "${ROOTFS_DIR}/etc/resolv.conf" ] || [ -L "${ROOTFS_DIR}/etc/resolv.conf" ]; then
    mv "${ROOTFS_DIR}/etc/resolv.conf" "${ROOTFS_DIR}/etc/resolv.conf.autd3-orig"
  fi
  cp -f /etc/resolv.conf "${ROOTFS_DIR}/etc/resolv.conf"
}

unmount_image() {
  say "unmounting"
  unmount_all
}

on_chroot() {
  capsh --drop=cap_setfcap "--chroot=${ROOTFS_DIR}/" -- -e "$@"
}
export -f on_chroot

in_chroot() {
  on_chroot << EOF
set -e
${1}
EOF
}

silence_chroot() {
  install -m 755 /dev/stdin "${ROOTFS_DIR}/usr/sbin/policy-rc.d" << 'EOF'
#!/bin/sh
exit 101
EOF
  mv "${ROOTFS_DIR}/sbin/start-stop-daemon" "${ROOTFS_DIR}/sbin/start-stop-daemon.autd3"
  install -m 755 /dev/stdin "${ROOTFS_DIR}/sbin/start-stop-daemon" << 'EOF'
#!/bin/sh
exit 0
EOF
}

restore_chroot() {
  rm -f "${ROOTFS_DIR}/usr/sbin/policy-rc.d"
  mv "${ROOTFS_DIR}/sbin/start-stop-daemon.autd3" "${ROOTFS_DIR}/sbin/start-stop-daemon"
}

adapt_base_image() {
  say "adapting the stock image to the appliance's defaults"

  echo "${TARGET_HOSTNAME}" > "${ROOTFS_DIR}/etc/hostname"
  sed -i "s/^\(127\.0\.1\.1[[:space:]]*\).*/\1${TARGET_HOSTNAME}/" "${ROOTFS_DIR}/etc/hosts"
  echo "LANG=${LOCALE_DEFAULT}" > "${ROOTFS_DIR}/etc/default/locale"
  sed -i "s/^XKBLAYOUT=.*/XKBLAYOUT=\"${KEYBOARD_KEYMAP}\"/" "${ROOTFS_DIR}/etc/default/keyboard"
  echo "${TIMEZONE_DEFAULT}" > "${ROOTFS_DIR}/etc/timezone"
  ln -snf "/usr/share/zoneinfo/${TIMEZONE_DEFAULT}" "${ROOTFS_DIR}/etc/localtime"

  rm -f "${ROOTFS_DIR}/etc/ssh/sshd_config.d/rename_user.conf"
  in_chroot "
if getent passwd pi > /dev/null && ! getent passwd '${FIRST_USER_NAME}' > /dev/null; then
    usermod --login '${FIRST_USER_NAME}' --home '/home/${FIRST_USER_NAME}' --move-home pi
    groupmod --new-name '${FIRST_USER_NAME}' pi
fi
chsh -s /bin/bash '${FIRST_USER_NAME}'
systemctl disable userconfig.service
systemctl enable ssh
"
  if [ "${AUTD3_LOCK_ACCOUNT}" != "1" ]; then
    in_chroot "echo '${FIRST_USER_NAME}:${FIRST_USER_PASS}' | chpasswd"
  fi

  if [ -n "${PUBKEY_SSH_FIRST_USER:-}" ]; then
    install -d -m 700 -o 1000 -g 1000 "${ROOTFS_DIR}/home/${FIRST_USER_NAME}/.ssh"
    install -m 600 -o 1000 -g 1000 /dev/stdin \
      "${ROOTFS_DIR}/home/${FIRST_USER_NAME}/.ssh/authorized_keys" \
      <<< "${PUBKEY_SSH_FIRST_USER}"
    install -m 644 /dev/stdin \
      "${ROOTFS_DIR}/etc/ssh/sshd_config.d/50-autd3-pubkey-only.conf" << 'EOF'
PasswordAuthentication no
KbdInteractiveAuthentication no
EOF
  fi

  say "dropping every kernel flavour but ${KERNEL}"
  in_chroot "
export DEBIAN_FRONTEND=noninteractive
drop=\$(dpkg-query -W -f='\${Package}\n' 'linux-image-*' 'linux-headers-*' 2> /dev/null |
        grep -v -- '-${KERNEL}\$' || true)
if [ -n \"\${drop}\" ]; then apt-get -y purge \${drop}; fi
apt-get -y autoremove --purge
dpkg-query -W -f='\${Package}\n' 'linux-image-*' | grep -q -- '-${KERNEL}\$'
"
  rm -f "${BOOT_DIR}"/kernel_2712.img "${BOOT_DIR}"/initramfs_2712
}

run_stage() {
  say "running $(basename "${STAGE_DIR}")"
  in_chroot "apt-get -o Acquire::Retries=3 -y update"

  local packages
  packages=$(sed 's/#.*//' "${STAGE_DIR}/packages" | tr '\n' ' ')
  if [ -n "${packages// /}" ]; then
    say "  installing the stage packages"
    in_chroot "
export DEBIAN_FRONTEND=noninteractive
apt-get -o Acquire::Retries=3 install -y ${packages}
"
  fi

  say "  running run.sh"
  (cd "${STAGE_DIR}" && ./run.sh)
}

cleanup_rootfs() {
  say "clearing the package cache and the build's traces"
  in_chroot "apt-get clean"
  rm -rf "${ROOTFS_DIR}/var/lib/apt/lists"/*
  install -d "${ROOTFS_DIR}/var/lib/apt/lists/partial"
  rm -f "${ROOTFS_DIR}/etc/resolv.conf"
  if [ -e "${ROOTFS_DIR}/etc/resolv.conf.autd3-orig" ] ||
    [ -L "${ROOTFS_DIR}/etc/resolv.conf.autd3-orig" ]; then
    mv "${ROOTFS_DIR}/etc/resolv.conf.autd3-orig" "${ROOTFS_DIR}/etc/resolv.conf"
  fi
  rm -f "${ROOTFS_DIR}/root/.bash_history"
  find "${ROOTFS_DIR}/var/log" -type f -exec truncate -s 0 {} +
}

zero_free_space() {
  say "zeroing the free blocks so the image compresses"
  local mount
  for mount in "${ROOTFS_DIR}" "${BOOT_DIR}"; do
    dd if=/dev/zero of="${mount}/.autd3-zero" bs=4M status=none 2> /dev/null || true
    rm -f "${mount}/.autd3-zero"
  done
  sync
}

grow_rootfs
mount_image
silence_chroot
adapt_base_image
run_stage
cleanup_rootfs
restore_chroot
unmount_image
shrink_rootfs
mount_filesystems
zero_free_space
unmount_all
finish_partition

say "built $(du -h "${IMAGE}" | cut -f1) of image"
