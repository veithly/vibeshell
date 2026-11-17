#!/bin/sh
set -eu
# Password-via-PAM and conservative RSA/AES-CTR policies exercise distinct routes.
/usr/sbin/sshd -D -e -p 23 -o PidFile=/run/sshd-pam.pid -o PasswordAuthentication=no &
/usr/sbin/sshd -D -e -p 24 -o PidFile=/run/sshd-compat.pid -o HostKey=/etc/ssh/ssh_host_rsa_key -o HostKeyAlgorithms=rsa-sha2-512,rsa-sha2-256 -o Ciphers=aes128-ctr -o KexAlgorithms=diffie-hellman-group14-sha256 &
exec /usr/sbin/sshd -D -e
