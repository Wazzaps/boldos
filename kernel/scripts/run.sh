#!/usr/bin/env bash

ROOT_PATH="$(dirname "$0")/.."
KERNEL="$(realpath "$1")"
cd "$ROOT_PATH" || exit
shift

MEM=256M
CPU_CORES=4
CPU_TYPE=cortex-a72

qemu-system-aarch64 \
  -machine virt -cpu $CPU_TYPE -smp $CPU_CORES -m $MEM \
  -nographic \
  -kernel "$KERNEL" -append "kernel params" -initrd "./initrd.bin" \
  -fsdev local,path=../rootfs,security_model=mapped-xattr,id=rootfs,readonly=on,multidevs=forbid \
  -device virtio-9p-device,fsdev=rootfs,mount_tag=rootfs \
  -gdb tcp::1234 "$@"
