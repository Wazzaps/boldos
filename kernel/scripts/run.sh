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
  -kernel "$KERNEL" -append "placeholder kernel params" -initrd "./initrd.bin" \
  -global virtio-mmio.force-legacy=false \
  -fw_cfg name=opt/org.boldos/initrd,file=./initrd.bin \
  -fsdev local,path=../rootfs,security_model=mapped-xattr,id=rootfs,readonly=on,multidevs=forbid \
  -device virtio-9p-device,fsdev=rootfs,mount_tag=rootfs \
  -blockdev driver=file,node-name=file0,filename="$(pwd)/initrd.bin",read-only=on \
  -blockdev driver=raw,node-name=hd0,file=file0,read-only=on \
  -device virtio-blk-device,drive=hd0,id=virtio-disk0 \
  -gdb tcp::1234 "$@"
