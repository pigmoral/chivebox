pub const SUPPORTED_TARGETS: &[(&str, &str)] = &[
    ("riscv64", "riscv64gc-unknown-linux-musl"),
    ("riscv", "riscv64gc-unknown-linux-musl"),
    ("arm64", "aarch64-unknown-linux-musl"),
    ("aarch64", "aarch64-unknown-linux-musl"),
    ("arm", "armv7-unknown-linux-musleabihf"),
    ("x86_64", "x86_64-unknown-linux-musl"),
    ("amd64", "x86_64-unknown-linux-musl"),
    ("x64", "x86_64-unknown-linux-musl"),
    ("i386", "i686-unknown-linux-musl"),
    ("i686", "i686-unknown-linux-musl"),
    ("x86", "i686-unknown-linux-musl"),
];

pub fn resolve_target(input: &str) -> crate::Result<String> {
    let input_lower = input.to_lowercase();

    for (short, full) in SUPPORTED_TARGETS {
        if *short == input_lower {
            return Ok(full.to_string());
        }
        if *full == input {
            return Ok(full.to_string());
        }
    }

    Err(crate::error::Error::UnknownTarget(input.to_string()))
}

pub fn get_short_name(triple: &str) -> String {
    for (short, full) in SUPPORTED_TARGETS {
        if *full == triple {
            return short.to_string();
        }
    }
    triple.to_string()
}
