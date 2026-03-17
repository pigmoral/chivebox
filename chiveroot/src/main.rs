use std::process::Command;

mod builder;
mod embedded;
mod error;
mod target;

use builder::InitramfsBuilder;
use clap::Parser;
use error::Result;

#[derive(Parser, Debug)]
#[command(name = "chiveroot")]
#[command(version = "0.1.0")]
#[command(
    about = "Build chivebox initramfs for embedded systems",
    long_about = "A tool to build initramfs containing chivebox (a BusyBox-style multi-call binary) with optional kernel modules and firmware."
)]
struct Args {
    #[arg(
        short,
        long,
        help = "Target architecture (e.g., riscv64, arm64, x86_64)"
    )]
    target: Option<String>,

    #[arg(short, long, help = "Output directory or file (default: /tmp)")]
    output: Option<String>,

    #[arg(short, long, help = "Kernel modules to include (file or directory)")]
    modules: Option<String>,

    #[arg(
        short,
        long,
        help = "Kernel version for modules directory (e.g., 5.15.0)"
    )]
    kernel_version: Option<String>,

    #[arg(long, help = "Firmware files to include (file or directory)")]
    firmware: Option<String>,

    #[arg(
        short,
        long,
        help = "Additional files to include (format: src:dst, can be repeated)"
    )]
    file: Vec<String>,

    #[arg(short, long, help = "Path to pre-built chivebox binary")]
    binary: Option<String>,

    #[arg(long, help = "Path to chivebox source directory")]
    source: Option<String>,

    #[arg(long, help = "List supported target architectures")]
    list_targets: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();

    if args.list_targets {
        println!("Supported targets:");
        for (short, full) in target::SUPPORTED_TARGETS {
            println!("  {:12} -> {}", short, full);
        }
        return Ok(());
    }

    let target = args
        .target
        .ok_or_else(|| error::Error::UnknownTarget("--target is required".to_string()))?;

    let target_triple = target::resolve_target(&target)?;

    println!("Building initramfs for target: {}", target_triple);

    check_cargo_zigbuild()?;

    let binary_path = if let Some(binary) = &args.binary {
        println!("Using provided binary: {}", binary);
        std::path::PathBuf::from(binary)
    } else if let Some(source) = &args.source {
        build_chivebox(std::path::Path::new(source), &target_triple)?
    } else {
        // Use embedded source
        let binary_path = embedded::build_chivebox_from_embedded(&target_triple)?;
        binary_path
    };

    let applets = if args.binary.is_some() {
        // Try to get applets from binary
        match embedded::get_applets_from_binary(&binary_path) {
            Ok(applets) => applets,
            Err(_) => {
                println!("Could not run binary to get applets, parsing from embedded source...");
                embedded::extract_embedded_source()?;
                embedded::parse_applets_from_source()?
            }
        }
    } else {
        // Binary was built from embedded source, parse applets from source
        embedded::parse_applets_from_source()?
    };

    let output_path = args.output.unwrap_or_else(|| "/tmp".to_string());

    let short_target = target::get_short_name(&target_triple);

    let builder = InitramfsBuilder::new(
        output_path,
        short_target,
        applets,
        args.modules,
        args.kernel_version,
        args.firmware,
        args.file,
    )?;

    let result = builder.build(&binary_path)?;
    println!("Initramfs created: {}", result.display());

    Ok(())
}

fn check_cargo_zigbuild() -> Result<()> {
    let output = Command::new("cargo-zigbuild").arg("--version").output();

    match output {
        Ok(o) if o.status.success() => {
            println!("cargo-zigbuild found");
            Ok(())
        }
        _ => Err(error::Error::MissingToolchain(
            "cargo-zigbuild not found. Please install it:\n\
                 cargo install cargo-zigbuild"
                .to_string(),
        )),
    }
}

fn build_chivebox(source_dir: &std::path::Path, target: &str) -> Result<std::path::PathBuf> {
    println!("Building chivebox from source...");

    let binary_dir = source_dir.join("target").join(target).join("release");

    if binary_dir.join("chivebox").exists() {
        println!("Using existing build at {:?}", binary_dir.join("chivebox"));
        return Ok(binary_dir.join("chivebox"));
    }

    println!("Building chivebox (this may take a while)...");

    let status = Command::new("cargo")
        .args(["zigbuild", "--release", "--target", target])
        .current_dir(source_dir)
        .status()?;

    if !status.success() {
        return Err(error::Error::BuildFailure(
            "chivebox build failed".to_string(),
        ));
    }

    Ok(binary_dir.join("chivebox"))
}
