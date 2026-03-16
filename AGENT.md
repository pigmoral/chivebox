# Chivebox Development Notes

## Project Focus

- `chivebox` is a Rust multi-call binary in the BusyBox style.
- Base applets such as `ls`, `cp`, `cat`, `mv`, `rm`, `mkdir`, `touch`, `pwd`, `echo`, `df`, and `sync` are primarily provided through uutils crates.
- Custom work is currently concentrated in:
  - Shell (`src/sh/rush/`) for serial-first RISC-V initramfs environments
  - Filesystem utilities (`src/volume_id/`, `src/blkid.rs`, `src/mount.rs`, `src/umount.rs`)
  - Early-init workflow (`src/init.rs`)

## Current Shell Direction

- The shell implementation lives under `src/sh/rush/`.
- `reedline` has been removed on purpose. The shell now uses a serial-first basic line editor that is more reliable on QEMU `ttyS0` and real board serial consoles.
- After every non-trivial code change, run at least `cargo check`. Add or update focused tests whenever behavior changes or a bug is fixed.
- When debugging shell behavior, prefer adding targeted, removable instrumentation that follows `busybox hush`'s tty/job-control model rather than speculative fallback logic.
- The shell is split into:
  - `src/sh/rush/mod.rs`: REPL orchestration
  - `src/sh/rush/input.rs`: basic line editor for serial/TTY input
  - `src/sh/rush/completion.rs`: completion logic independent of any readline library
  - `src/sh/rush/shell.rs`: tokenizer, parser, expansion, execution, builtins, redirections, and pipelines

## Implemented Shell Capabilities

- Basic interactive prompt using current working directory and standard shell markers:
  - root prompt ends with `#`
  - non-root prompt ends with `$`
- Continuation prompt `> ` for incomplete input.
- Line editing features in basic mode:
  - Enter
  - Backspace
  - Ctrl-C for interactive line cancel
  - Ctrl-D on empty line to exit
  - Ctrl-L to clear screen
  - Tab completion
  - Up/Down arrow keys for command history navigation (in-memory, max 100 entries)
  - Left/Right arrow keys for cursor movement within the line
  - Insert and delete characters at any cursor position
- Completion behavior:
  - command completion for builtins, bundled applets, and `PATH`
  - path completion with preserved full insertion path
  - `cd` only completes directories
  - display names are basename-style (`cat`, `chivebox`, `child/`) instead of full paths
  - multiple candidates are printed in a compact column layout instead of one-per-line
- Parser/executor features:
  - quoting and escaping
  - `;`, `&&`, `||`, `|`
  - `<`, `>`, `>>`
  - environment assignments before commands
  - `$VAR`, `${VAR}`, `$?`
  - builtins: `cd`, `exit`, `export`, `pwd`, `unset`

## Filesystem Utilities

### volume_id Module (`src/volume_id/mod.rs`)

A filesystem detection module that probes block device superblocks to identify filesystem types, UUIDs, and labels. Supports:
- ext2/ext3/ext4 (distinguishes versions by feature flags)
- xfs
- btrfs
- f2fs
- squashfs
- vfat
- ntfs

The module reads magic numbers and metadata from well-known superblock offsets, similar to busybox's `util-linux/volume_id/` implementation.

### blkid (`src/blkid.rs`)

Block device identification utility using the volume_id module. Outputs `DEVNAME: UUID="..." TYPE="..." LABEL="..."` format. When called without arguments, scans `/dev` for block devices.

### mount (`src/mount.rs`)

Mount command with automatic filesystem type detection. When `-t` is not specified, uses volume_id to probe the device's filesystem type instead of blindly iterating through `/proc/filesystems`. This avoids unnecessary kernel warning messages from failed mount attempts.

### umount (`src/umount.rs`)

Unmount utility supporting:
- `-f` force unmount (lazy)
- `-l` lazy unmount
- `-r` remount read-only on failure
- `-v` verbose
- `-a` unmount all
- `-t <type>` filter by filesystem type

Handles trailing slashes in mount point paths correctly.

### sync

Synchronizes filesystem caches via `uu_sync`.

## Signal Handling Notes

- Child processes restore default `SIGINT` handling before running commands.
- Interactive execution is moving toward hush-style tty ownership: the shell needs to hand the controlling tty to the foreground process group and reclaim it afterwards.
- In `rdinit=/bin/sh` and PID 1 scenarios, verify whether the shell has a controlling tty before assuming job control is available. Reference `busybox ash` and `cttyhack` behavior for acquiring ctty.
- Signal exits are mapped to shell-style statuses such as `130` for `SIGINT`.
- Always re-verify Ctrl-C behavior on both host PTY and QEMU serial after touching input, termios, or process execution code.

## Initramfs / Build Notes

### Init Program (src/init.rs)

The init program is the first process run by the kernel (PID 1). It handles:
- Signal initialization (ignores most signals, except SIGCHLD which must work for wait())
- Console initialization (opens /dev/console and duplicates to stdin/stdout/stderr)
- Filesystem mounting:
  - `/proc` (procfs)
  - `/dev` (devtmpfs)
  - `/sys` (sysfs)
  - `/tmp` (tmpfs)
- Environment setup (PATH, SHELL, USER, TERM)
- Shell spawning loop with auto-restart

**Important**: Do NOT set `signal(SIGCHLD, SIG_IGN)` - this causes the kernel to auto-reap children, making `wait()` fail with "waitpid failed". Use the default signal handling or a custom handler.

### Adding New uutils Commands

When adding a new command from uutils (e.g., `uu_df`), follow these steps:

1. **Add dependency** in `Cargo.toml`:
```toml
uu_<command> = "0.7"
```

2. **Register applet** in `src/applets.rs`:
```rust
Applet {
    name: "<command>",
    help: "Help text",
    main: uu_<command>::uumain,
},
```

Note: Do NOT create a separate entry file like `src/<command>.rs` - just reference `uu_<command>::uumain` directly in `applets.rs`.

### Host Build
- Host release build: `cargo build --release`
- RISC-V musl release (use zig linker): `cargo zigbuild --release --target riscv64gc-unknown-linux-musl`
- Plain `cargo build --release --target riscv64gc-unknown-linux-musl` may fail on hosts without a working RISC-V musl linker.

### Creating Initramfs

**Important**: Do NOT create device nodes in initramfs using `mknod`. The Linux kernel's devtmpfs will automatically create proper device nodes (`/dev/console`, `/dev/tty`, `/dev/ttyS0`, `/dev/null`, etc.) at boot time. Creating fake device nodes (even with fakeroot) will result in regular files instead of real device nodes, causing I/O failures.

Steps to create a working initramfs:
```bash
# Create directory structure
mkdir -p initramfs/{bin,sbin,etc,proc,sys,root}

# Copy chivebox binary
cp target/riscv64gc-unknown-linux-musl/release/chivebox initramfs/bin/

# Create symlinks for applets (required for initramfs)
cd initramfs/bin
for cmd in sh cat cp echo ls mkdir mount mv pwd rm touch blkid umount sync init; do
    ln -sf chivebox $cmd
done

# Create /init symlink (required - kernel looks for this)
cd initramfs
ln -s bin/chivebox init

# Package initramfs (do NOT create device nodes manually)
cd initramfs
find . -print | cpio -o --format=newc > ../initramfs.cpio
gzip -c ../initramfs.cpio > ../initramfs.cpio.gz
```

### Running QEMU with Initramfs

```bash
qemu-system-riscv64 \
  -M virt \
  -m 256M \
  -kernel Image \
  -initrd initramfs.cpio.gz \
  -append "console=ttyS0" \
  -nographic
```

Note: The kernel's devtmpfs will automatically create `/dev/console`, `/dev/tty`, `/dev/ttyS0`, `/dev/null`, etc. from the kernel command line (`console=ttyS0`).

## Known Gaps / Next Work

- Foreground job control and tty handoff are critical areas and should be kept aligned with `busybox hush` behavior.
- `Ctrl-C` handling for all serial/QEMU combinations should be re-verified after each input-layer change.
- Missing shell features compared with `busybox hush` still include things like:
  - `2>` / `2>>` / `2>&1`
  - `.` / `source`
  - `read`
  - heredocs
  - command substitution
  - control flow (`if`, `while`, `for`, `case`)

## General Guidelines

- Use English for all code comments
- Reference busybox source code for implementation patterns
- Document development decisions in this file

## Reference Priority

- First reference: `../busybox/shell/hush.c` and `../busybox/shell/hush_doc.txt`
- Second reference: `../brush` basic/minimal interactive input code
- The design target is not full bash compatibility. The priority is a small, usable, serial-friendly shell suitable for initramfs and embedded systems.
