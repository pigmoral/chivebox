# ChiveBox

A BusyBox-style multi-call binary for embedded initramfs, written in Rust.

## Features

- applets from uutils/coreutils and uutils/util-linux
- Custom implementations: `init`, `mount`, `umount`, `blkid`
- Simple shell (`rush`) with command history and tab completion
- Built-in `chiveroot` tool for generating initramfs with embedded chivebox source

## Requirements

- `cargo-zigbuild` for cross-compilation

```bash
cargo install cargo-zigbuild
```

## Usage

### chiveroot - Build initramfs

`chiveroot` generates initramfs containing chivebox. It embeds chivebox source code, so you don't need git or separate source download.

```bash
cargo install --git https://github.com/pigmoral/chivebox chiveroot
```

```bash
# Build initramfs for RISC-V
chiveroot --target riscv64

# Include kernel modules and firmware
chiveroot --target riscv64 --modules /path/to/modules --firmware /path/to/firmware

# Use pre-built binary
chiveroot --target riscv64 --binary /path/to/chivebox # chivebox binary path

# Use local chivebox source instead of embedded source
chiveroot --target riscv64 --source /path/to/chivebox/ # chivebox source path

# Add extra files (such as extra binary)
chiveroot --target riscv64 --file /path/to/binary:/bin/binary
```

### chivebox - Multi-call binary

```bash
# Run as specific applet
chivebox ls -la
chivebox cat file.txt
chivebox sh

# List available applets
chivebox --list
```

## Build ChiveBox from source

```bash
# Clone the repository
git clone https://github.com/pigmoral/chivebox
cd chivebox

# Build chivebox for RISC-V
cargo zigbuild --release --target riscv64gc-unknown-linux-musl

# Build chiveroot
cargo build --release -p chiveroot

# Or build entire workspace (including chivebox for your host architecture)
cargo build --release
```

## Acknowledgments

- [busybox](https://busybox.net/) - Inspiration for multi-call binary design
- [uutils/coreutils](https://github.com/uutils/coreutils) - Coreutils in Rust
- [uutils/util-linux](https://github.com/uutils/util-linux) - Util-linux in Rust
- [u-root](https://github.com/u-root/u-root) - A Go-based initramfs toolchain
