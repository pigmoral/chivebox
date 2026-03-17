use crate::error::{Error, Result};
use flate2::read::GzDecoder;
use regex::Regex;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

const CHIVEBOX_SOURCE_TAR: &[u8] = include_bytes!(env!("CHIVEBOX_SOURCE_TAR"));

pub fn get_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("chiveroot")
}

pub fn get_source_dir() -> PathBuf {
    get_cache_dir().join("chivebox-source")
}

pub fn get_binary_dir(target: &str) -> PathBuf {
    get_cache_dir().join("binaries").join(target)
}

pub fn extract_embedded_source() -> Result<PathBuf> {
    let source_dir = get_source_dir();

    if source_dir.exists() {
        println!("Using cached source at {:?}", source_dir);
        return Ok(source_dir);
    }

    println!("Extracting embedded chivebox source...");

    fs::create_dir_all(&source_dir)?;

    let cursor = Cursor::new(CHIVEBOX_SOURCE_TAR);
    let decoder = GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(decoder);

    archive.unpack(&source_dir)?;

    println!("Extracted source to {:?}", source_dir);
    Ok(source_dir)
}

pub fn build_chivebox_from_embedded(target: &str) -> Result<PathBuf> {
    let source_dir = extract_embedded_source()?;
    let binary_dir = get_binary_dir(target);
    let binary_path = binary_dir.join("chivebox");

    println!("Building chivebox for {}...", target);

    fs::create_dir_all(&binary_dir)?;

    let status = Command::new("cargo")
        .args(["zigbuild", "--release", "--target", target])
        .current_dir(&source_dir)
        .status()?;

    if !status.success() {
        return Err(Error::BuildFailure("chivebox build failed".to_string()));
    }

    let built_binary = source_dir
        .join("target")
        .join(target)
        .join("release")
        .join("chivebox");

    if built_binary.exists() {
        fs::copy(&built_binary, &binary_path)?;
        println!("Copied binary to {:?}", binary_path);
        Ok(binary_path)
    } else {
        Err(Error::BuildFailure(format!(
            "Binary not found at {:?}",
            built_binary
        )))
    }
}

pub fn parse_applets_from_source() -> Result<Vec<String>> {
    let source_dir = get_source_dir();
    parse_applets_from_dir(&source_dir)
}

pub fn parse_applets_from_dir(source_dir: &Path) -> Result<Vec<String>> {
    let applets_path = source_dir.join("src").join("applets.rs");

    if !applets_path.exists() {
        return Err(Error::AppletListFailed(
            "applets.rs not found in source directory".to_string(),
        ));
    }

    let content = fs::read_to_string(&applets_path)?;

    let re = Regex::new(r#"name:\s*"([^"]+)""#).unwrap();

    let applets: Vec<String> = re
        .captures_iter(&content)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect();

    println!("Found {} applets from source", applets.len());
    Ok(applets)
}

pub fn get_applets_from_binary(binary_path: &Path) -> Result<Vec<String>> {
    println!("Getting applet list from chivebox...");

    let output = Command::new(binary_path).arg("--list").output()?;

    if !output.status.success() {
        return Err(Error::AppletListFailed(
            "chivebox --list failed".to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let applets: Vec<String> = stdout
        .lines()
        .filter_map(|line| {
            let name = line.split_whitespace().next()?;
            if name.starts_with("--") || name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect();

    println!("Found {} applets: {:?}", applets.len(), applets);
    Ok(applets)
}
