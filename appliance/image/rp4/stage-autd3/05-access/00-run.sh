#!/bin/bash -e

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
