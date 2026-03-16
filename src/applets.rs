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
        name: "cp",
        help: "Copy files",
        main: uu_cp::uumain,
    },
    Applet {
        name: "df",
        help: "Show disk usage",
        main: uu_df::uumain,
    },
    Applet {
        name: "echo",
        help: "Display text",
        main: uu_echo::uumain,
    },
    Applet {
        name: "init",
        help: "Initialize system",
        main: init::main,
    },
    Applet {
        name: "ls",
        help: "List directory contents",
        main: uu_ls::uumain,
    },
    Applet {
        name: "mkdir",
        help: "Create directories",
        main: uu_mkdir::uumain,
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
        name: "pwd",
        help: "Print working directory",
        main: uu_pwd::uumain,
    },
    Applet {
        name: "rm",
        help: "Remove files",
        main: uu_rm::uumain,
    },
    Applet {
        name: "sh",
        help: "Simple shell",
        main: sh::main,
    },
    Applet {
        name: "sync",
        help: "Synchronize filesystem caches",
        main: uu_sync::uumain,
    },
    Applet {
        name: "touch",
        help: "Create empty files",
        main: uu_touch::uumain,
    },
    Applet {
        name: "umount",
        help: "Unmount filesystems",
        main: umount::main,
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
