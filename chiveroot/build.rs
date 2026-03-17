use std::env;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("chivebox-source.tar.gz");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let chivebox_dir = Path::new(&manifest_dir).parent().unwrap();

    let file = File::create(&dest_path).expect("Failed to create output file");
    let mut writer = BufWriter::new(file);

    let mut encoder = flate2::write::GzEncoder::new(&mut writer, flate2::Compression::default());
    let mut tar = tar::Builder::new(&mut encoder);

    // Process and add Cargo.toml (strip [workspace] section)
    let cargo_toml_path = chivebox_dir.join("Cargo.toml");
    let cargo_toml_content =
        fs::read_to_string(&cargo_toml_path).expect("Failed to read Cargo.toml");
    let stripped_toml = strip_workspace_section(&cargo_toml_content);

    let mut header = tar::Header::new_gnu();
    header.set_size(stripped_toml.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "Cargo.toml", io::Cursor::new(stripped_toml))
        .expect("Failed to add Cargo.toml");

    // Add Cargo.lock
    let cargo_lock_path = chivebox_dir.join("Cargo.lock");
    if cargo_lock_path.exists() {
        tar.append_path_with_name(&cargo_lock_path, "Cargo.lock")
            .expect("Failed to add Cargo.lock");
    }

    // Add src directory
    let src_dir = chivebox_dir.join("src");
    if src_dir.exists() {
        tar.append_dir_all("src", &src_dir)
            .expect("Failed to add src directory");
    }

    tar.finish().expect("Failed to finish tar archive");
    drop(tar);
    encoder.finish().expect("Failed to finish gzip encoding");
    writer.flush().expect("Failed to flush writer");

    println!("cargo:rerun-if-changed=../Cargo.toml");
    println!("cargo:rerun-if-changed=../Cargo.lock");
    println!("cargo:rerun-if-changed=../src");
    println!(
        "cargo:rustc-env=CHIVEBOX_SOURCE_TAR={}",
        dest_path.display()
    );
}

fn strip_workspace_section(content: &str) -> String {
    let mut result = String::new();
    let mut in_workspace = false;

    for line in content.lines() {
        if line.starts_with("[workspace") {
            in_workspace = true;
            continue;
        }
        if line.starts_with("[package") {
            in_workspace = false;
        } else if line.starts_with('[') && !line.starts_with("[profile") {
            in_workspace = false;
        }

        if in_workspace {
            continue;
        }

        result.push_str(line);
        result.push('\n');
    }

    result
}
