use std::env;
use std::iter;

use crate::{blkid, init, mount, sh, umount};

pub struct Applet {
    pub name: &'static str,
    pub help: &'static str,
    pub main: fn(args: iter::Skip<env::ArgsOs>) -> i32,
}

pub const APPLETS: &[Applet] = &[
    Applet {
        name: "arch",
        help: "Display machine architecture",
        main: uu_arch::uumain,
    },
    Applet {
        name: "b2sum",
        help: "Print or check BLAKE2b checksums",
        main: uu_b2sum::uumain,
    },
    Applet {
        name: "base32",
        help: "Encode/decode base32",
        main: uu_base32::uumain,
    },
    Applet {
        name: "base64",
        help: "Encode/decode base64",
        main: uu_base64::uumain,
    },
    Applet {
        name: "basename",
        help: "Print filename without directory",
        main: uu_basename::uumain,
    },
    Applet {
        name: "basenc",
        help: "Encode/decode data",
        main: uu_basenc::uumain,
    },
    Applet {
        name: "blkid",
        help: "Print UUID and TYPE of block devices",
        main: blkid::main,
    },
    Applet {
        name: "cat",
        help: "Concatenate and print files",
        main: uu_cat::uumain,
    },
    Applet {
        name: "chgrp",
        help: "Change group ownership",
        main: uu_chgrp::uumain,
    },
    Applet {
        name: "chmod",
        help: "Change file modes",
        main: uu_chmod::uumain,
    },
    Applet {
        name: "chown",
        help: "Change owner and group",
        main: uu_chown::uumain,
    },
    Applet {
        name: "chroot",
        help: "Run command with new root",
        main: uu_chroot::uumain,
    },
    Applet {
        name: "cksum",
        help: "Print CRC and byte counts",
        main: uu_cksum::uumain,
    },
    Applet {
        name: "comm",
        help: "Compare sorted files",
        main: uu_comm::uumain,
    },
    Applet {
        name: "cp",
        help: "Copy files",
        main: uu_cp::uumain,
    },
    Applet {
        name: "csplit",
        help: "Split file by context",
        main: uu_csplit::uumain,
    },
    Applet {
        name: "cut",
        help: "Print selected parts of lines",
        main: uu_cut::uumain,
    },
    Applet {
        name: "date",
        help: "Print or set system date/time",
        main: uu_date::uumain,
    },
    Applet {
        name: "dd",
        help: "Convert and copy files",
        main: uu_dd::uumain,
    },
    Applet {
        name: "df",
        help: "Show disk usage",
        main: uu_df::uumain,
    },
    Applet {
        name: "dir",
        help: "List directory contents",
        main: uu_dir::uumain,
    },
    Applet {
        name: "dircolors",
        help: "Set LS_COLORS",
        main: uu_dircolors::uumain,
    },
    Applet {
        name: "dirname",
        help: "Print directory from path",
        main: uu_dirname::uumain,
    },
    Applet {
        name: "du",
        help: "Estimate file space usage",
        main: uu_du::uumain,
    },
    Applet {
        name: "echo",
        help: "Display text",
        main: uu_echo::uumain,
    },
    Applet {
        name: "env",
        help: "Run in modified environment",
        main: uu_env::uumain,
    },
    Applet {
        name: "expand",
        help: "Convert tabs to spaces",
        main: uu_expand::uumain,
    },
    Applet {
        name: "expr",
        help: "Evaluate expressions",
        main: uu_expr::uumain,
    },
    Applet {
        name: "factor",
        help: "Print prime factors",
        main: uu_factor::uumain,
    },
    Applet {
        name: "false",
        help: "Return false",
        main: uu_false::uumain,
    },
    Applet {
        name: "fmt",
        help: "Reformat paragraph text",
        main: uu_fmt::uumain,
    },
    Applet {
        name: "fold",
        help: "Wrap lines",
        main: uu_fold::uumain,
    },
    Applet {
        name: "groups",
        help: "Print group memberships",
        main: uu_groups::uumain,
    },
    Applet {
        name: "head",
        help: "Print first lines",
        main: uu_head::uumain,
    },
    Applet {
        name: "hostid",
        help: "Print host identifier",
        main: uu_hostid::uumain,
    },
    Applet {
        name: "hostname",
        help: "Print or set hostname",
        main: uu_hostname::uumain,
    },
    Applet {
        name: "id",
        help: "Print user/group IDs",
        main: uu_id::uumain,
    },
    Applet {
        name: "init",
        help: "Initialize system",
        main: init::main,
    },
    Applet {
        name: "install",
        help: "Copy files with attributes",
        main: uu_install::uumain,
    },
    Applet {
        name: "join",
        help: "Join lines on common field",
        main: uu_join::uumain,
    },
    Applet {
        name: "kill",
        help: "Send signal to process",
        main: uu_kill::uumain,
    },
    Applet {
        name: "link",
        help: "Create hard link",
        main: uu_link::uumain,
    },
    Applet {
        name: "ln",
        help: "Create links",
        main: uu_ln::uumain,
    },
    Applet {
        name: "logname",
        help: "Print login name",
        main: uu_logname::uumain,
    },
    Applet {
        name: "ls",
        help: "List directory contents",
        main: uu_ls::uumain,
    },
    Applet {
        name: "md5sum",
        help: "Print MD5 checksums",
        main: uu_md5sum::uumain,
    },
    Applet {
        name: "mkdir",
        help: "Create directories",
        main: uu_mkdir::uumain,
    },
    Applet {
        name: "mkfifo",
        help: "Create named pipes",
        main: uu_mkfifo::uumain,
    },
    Applet {
        name: "mknod",
        help: "Create special files",
        main: uu_mknod::uumain,
    },
    Applet {
        name: "mktemp",
        help: "Create temporary file",
        main: uu_mktemp::uumain,
    },
    Applet {
        name: "more",
        help: "Page through text",
        main: uu_more::uumain,
    },
    Applet {
        name: "mount",
        help: "Mount a filesystem",
        main: mount::main,
    },
    Applet {
        name: "mv",
        help: "Move files",
        main: uu_mv::uumain,
    },
    Applet {
        name: "nice",
        help: "Run with modified priority",
        main: uu_nice::uumain,
    },
    Applet {
        name: "nl",
        help: "Number lines",
        main: uu_nl::uumain,
    },
    Applet {
        name: "nohup",
        help: "Run immune to hangups",
        main: uu_nohup::uumain,
    },
    Applet {
        name: "nproc",
        help: "Print number of CPUs",
        main: uu_nproc::uumain,
    },
    Applet {
        name: "numfmt",
        help: "Format numbers",
        main: uu_numfmt::uumain,
    },
    Applet {
        name: "od",
        help: "Dump files in octal",
        main: uu_od::uumain,
    },
    Applet {
        name: "paste",
        help: "Merge lines of files",
        main: uu_paste::uumain,
    },
    Applet {
        name: "pathchk",
        help: "Check path validity",
        main: uu_pathchk::uumain,
    },
    Applet {
        name: "pinky",
        help: "Print user information",
        main: uu_pinky::uumain,
    },
    Applet {
        name: "pr",
        help: "Format for printing",
        main: uu_pr::uumain,
    },
    Applet {
        name: "printenv",
        help: "Print environment variables",
        main: uu_printenv::uumain,
    },
    Applet {
        name: "printf",
        help: "Format and print",
        main: uu_printf::uumain,
    },
    Applet {
        name: "ptx",
        help: "Produce permuted index",
        main: uu_ptx::uumain,
    },
    Applet {
        name: "pwd",
        help: "Print working directory",
        main: uu_pwd::uumain,
    },
    Applet {
        name: "readlink",
        help: "Print resolved symlink",
        main: uu_readlink::uumain,
    },
    Applet {
        name: "realpath",
        help: "Print resolved path",
        main: uu_realpath::uumain,
    },
    Applet {
        name: "rm",
        help: "Remove files",
        main: uu_rm::uumain,
    },
    Applet {
        name: "rmdir",
        help: "Remove directories",
        main: uu_rmdir::uumain,
    },
    Applet {
        name: "seq",
        help: "Print sequence of numbers",
        main: uu_seq::uumain,
    },
    Applet {
        name: "sh",
        help: "Simple shell",
        main: sh::main,
    },
    Applet {
        name: "sha1sum",
        help: "Print SHA1 checksums",
        main: uu_sha1sum::uumain,
    },
    Applet {
        name: "sha224sum",
        help: "Print SHA224 checksums",
        main: uu_sha224sum::uumain,
    },
    Applet {
        name: "sha256sum",
        help: "Print SHA256 checksums",
        main: uu_sha256sum::uumain,
    },
    Applet {
        name: "sha384sum",
        help: "Print SHA384 checksums",
        main: uu_sha384sum::uumain,
    },
    Applet {
        name: "sha512sum",
        help: "Print SHA512 checksums",
        main: uu_sha512sum::uumain,
    },
    Applet {
        name: "shred",
        help: "Securely delete files",
        main: uu_shred::uumain,
    },
    Applet {
        name: "shuf",
        help: "Shuffle lines",
        main: uu_shuf::uumain,
    },
    Applet {
        name: "sleep",
        help: "Pause execution",
        main: uu_sleep::uumain,
    },
    Applet {
        name: "sort",
        help: "Sort lines",
        main: uu_sort::uumain,
    },
    Applet {
        name: "split",
        help: "Split files",
        main: uu_split::uumain,
    },
    Applet {
        name: "stat",
        help: "Display file status",
        main: uu_stat::uumain,
    },
    Applet {
        name: "stty",
        help: "Change terminal settings",
        main: uu_stty::uumain,
    },
    Applet {
        name: "sum",
        help: "Print checksum and block count",
        main: uu_sum::uumain,
    },
    Applet {
        name: "sync",
        help: "Synchronize filesystem caches",
        main: uu_sync::uumain,
    },
    Applet {
        name: "tac",
        help: "Reverse lines",
        main: uu_tac::uumain,
    },
    Applet {
        name: "tail",
        help: "Print last lines",
        main: uu_tail::uumain,
    },
    Applet {
        name: "tee",
        help: "Read stdin, write files and stdout",
        main: uu_tee::uumain,
    },
    Applet {
        name: "test",
        help: "Check file types and values",
        main: uu_test::uumain,
    },
    Applet {
        name: "timeout",
        help: "Run with time limit",
        main: uu_timeout::uumain,
    },
    Applet {
        name: "touch",
        help: "Create empty files",
        main: uu_touch::uumain,
    },
    Applet {
        name: "tr",
        help: "Translate characters",
        main: uu_tr::uumain,
    },
    Applet {
        name: "true",
        help: "Return true",
        main: uu_true::uumain,
    },
    Applet {
        name: "truncate",
        help: "Shrink/extend files",
        main: uu_truncate::uumain,
    },
    Applet {
        name: "tsort",
        help: "Topological sort",
        main: uu_tsort::uumain,
    },
    Applet {
        name: "tty",
        help: "Print terminal name",
        main: uu_tty::uumain,
    },
    Applet {
        name: "umount",
        help: "Unmount filesystems",
        main: umount::main,
    },
    Applet {
        name: "uname",
        help: "Print system information",
        main: uu_uname::uumain,
    },
    Applet {
        name: "unexpand",
        help: "Convert spaces to tabs",
        main: uu_unexpand::uumain,
    },
    Applet {
        name: "uniq",
        help: "Filter duplicate lines",
        main: uu_uniq::uumain,
    },
    Applet {
        name: "unlink",
        help: "Remove single file",
        main: uu_unlink::uumain,
    },
    Applet {
        name: "uptime",
        help: "Show system uptime",
        main: uu_uptime::uumain,
    },
    Applet {
        name: "users",
        help: "Print logged in users",
        main: uu_users::uumain,
    },
    Applet {
        name: "vdir",
        help: "List directory contents",
        main: uu_vdir::uumain,
    },
    Applet {
        name: "wc",
        help: "Print line/word/byte counts",
        main: uu_wc::uumain,
    },
    Applet {
        name: "who",
        help: "Print logged in users",
        main: uu_who::uumain,
    },
    Applet {
        name: "whoami",
        help: "Print current username",
        main: uu_whoami::uumain,
    },
    Applet {
        name: "yes",
        help: "Print string repeatedly",
        main: uu_yes::uumain,
    },
];

pub fn find_applet(name: &str) -> Option<&'static Applet> {
    APPLETS
        .binary_search_by(|a| a.name.cmp(name))
        .ok()
        .map(|i| &APPLETS[i])
}

pub fn list_applets() -> &'static [Applet] {
    APPLETS
}
