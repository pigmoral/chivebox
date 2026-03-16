use std::env;
use std::fs;
use std::iter;

use crate::volume_id;

pub fn main(args: iter::Skip<env::ArgsOs>) -> i32 {
    let args: Vec<String> = args
        .skip(1)
        .map(|s| s.to_str().unwrap_or("").to_string())
        .collect();

    if !args.is_empty() && (args[0] == "-h" || args[0] == "--help") {
        print_usage();
        return 0;
    }

    let devices: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    if devices.is_empty() {
        return scan_all_devices();
    }

    let mut failed = false;
    for device in &devices {
        if let Some(info) = volume_id::probe_device(device) {
            print_device_info(device, &info);
        } else {
            failed = true;
        }
    }

    if failed {
        1
    } else {
        0
    }
}

fn print_usage() {
    println!("Usage: blkid [DEVICE]...");
    println!("Print UUID, TYPE, and LABEL of block devices");
}

fn scan_all_devices() -> i32 {
    let mut found = false;

    if let Ok(entries) = fs::read_dir("/dev") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if name.starts_with("sd")
                || name.starts_with("hd")
                || name.starts_with("vd")
                || name.starts_with("nvme")
                || name.starts_with("mmcblk")
                || name.starts_with("loop")
            {
                let path = entry.path();
                if let Some(info) = volume_id::probe_device(path.to_str().unwrap_or("")) {
                    print_device_info(path.to_str().unwrap_or(""), &info);
                    found = true;
                }

                scan_partitions(&path);
            }
        }
    }

    if found {
        0
    } else {
        1
    }
}

fn scan_partitions(device_path: &std::path::Path) {
    let dev_name = device_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    if let Ok(entries) = fs::read_dir("/sys/block") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if name == dev_name {
                let partitions_path = entry.path();
                if let Ok(part_entries) = fs::read_dir(&partitions_path) {
                    for part_entry in part_entries.flatten() {
                        let part_name = part_entry.file_name();
                        let part_name = part_name.to_string_lossy();

                        if part_name.starts_with(&*dev_name) && part_name != dev_name {
                            let dev_path = format!("/dev/{}", part_name);
                            if let Some(info) = volume_id::probe_device(&dev_path) {
                                print_device_info(&dev_path, &info);
                            }
                        }
                    }
                }
                break;
            }
        }
    }
}

fn print_device_info(device: &str, info: &volume_id::FsInfo) {
    let mut parts = Vec::new();

    if !info.uuid.is_empty() {
        parts.push(format!("UUID=\"{}\"", info.uuid));
    }
    if !info.fs_type.is_empty() {
        parts.push(format!("TYPE=\"{}\"", info.fs_type));
    }
    if !info.label.is_empty() {
        parts.push(format!("LABEL=\"{}\"", info.label));
    }

    if !parts.is_empty() {
        println!("{}: {}", device, parts.join(" "));
    }
}
