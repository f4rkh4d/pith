#!/usr/bin/env bash
# wrapper invoked by `cargo run`. cargo passes the freshly-built kernel
# elf as $1. we hand it to qemu's virt machine + opensbi.
set -euo pipefail

KERNEL="${1:-target/riscv64gc-unknown-none-elf/debug/pith}"
[ -f "$KERNEL" ] || { echo "kernel not built: $KERNEL"; exit 1; }

# qemu-system-riscv64 is the riscv64 system emulator. opensbi-rv64-generic
# ships with most distros' qemu packages and is the default firmware for
# the virt machine, so we don't bind it explicitly.
exec qemu-system-riscv64 \
    -machine virt \
    -cpu rv64 \
    -smp 1 \
    -m 128M \
    -nographic \
    -bios default \
    -kernel "$KERNEL"
