# ChiveBox

A BusyBox-style multi-call binary for embedded initramfs, written in Rust.

## Features

- applets from uutils/coreutils and uutils/util-linux
- Custom implementations: `init`, `mount`, `umount`, `blkid`
- Simple shell (`rush`) with command history and tab completion

## Requirements

- `cargo-zigbuild` for cross-compilation

Install with:

```bash
cargo install cargo-zigbuild
```

## Building

```bash
cargo zigbuild --release --target riscv64gc-unknown-linux-musl
```

## Acknowledgments

- [busybox](https://busybox.net/) - Inspiration for multi-call binary design
- [uutils/coreutils](https://github.com/uutils/coreutils) - Coreutils in Rust
- [uutils/util-linux](https://github.com/uutils/util-linux) - Util-linux in Rust
- [u-root](https://github.com/u-root/u-root) - A Go-based initramfs toolchain
