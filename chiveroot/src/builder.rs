use crate::error::Result;
use chrono::Local;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct InitramfsBuilder {
    output_path: PathBuf,
    applets: Vec<String>,
    modules_path: Option<String>,
    kernel_version: Option<String>,
    firmware_path: Option<String>,
    extra_files: Vec<String>,
}

impl InitramfsBuilder {
    pub fn new(
        output: String,
        short_target: String,
        applets: Vec<String>,
        modules_path: Option<String>,
        kernel_version: Option<String>,
        firmware_path: Option<String>,
        extra_files: Vec<String>,
    ) -> Result<Self> {
        let output_path = PathBuf::from(&output);
        let is_dir = output_path.exists() && output_path.is_dir();

        // If it's a directory, generate default filename with target and date
        let output_path = if is_dir {
            let date = Local::now().format("%Y%m%d_%H%M").to_string();
            output_path.join(format!("initramfs_{}_{}.cpio.gz", short_target, date))
        } else {
            output_path
        };

        Ok(Self {
            output_path,
            applets,
            modules_path,
            kernel_version,
            firmware_path,
            extra_files,
        })
    }

    pub fn build(&self, binary_path: &Path) -> Result<PathBuf> {
        let temp_dir = tempfile::tempdir()?;
        let root = temp_dir.path();

        self.create_directories(root)?;
        self.copy_binary(root, binary_path)?;
        self.create_symlinks(root)?;
        self.copy_modules(root)?;
        self.copy_firmware(root)?;
        self.copy_extra_files(root)?;
        self.create_init_link(root)?;

        self.package(root)?;

        let final_path = self.output_path.clone();
        Ok(final_path)
    }

    fn create_directories(&self, root: &Path) -> Result<()> {
        let dirs = [
            "bin",
            "lib/modules",
            "lib/firmware",
            "dev",
            "proc",
            "sys",
            "root",
            "etc",
            "tmp",
            "mnt",
        ];

        for dir in &dirs {
            fs::create_dir_all(root.join(dir))?;
        }

        Ok(())
    }

    fn copy_binary(&self, root: &Path, binary_path: &Path) -> Result<()> {
        let dest = root.join("bin").join("chivebox");
        fs::copy(binary_path, &dest)?;
        println!("Copied chivebox to {:?}", dest);
        Ok(())
    }

    fn create_symlinks(&self, root: &Path) -> Result<()> {
        let bin_dir = root.join("bin");

        for applet in &self.applets {
            let link_path = bin_dir.join(applet);
            if link_path.exists() {
                continue;
            }
            std::os::unix::fs::symlink("chivebox", &link_path)?;
            println!("Created symlink: {} -> chivebox", applet);
        }

        Ok(())
    }

    fn copy_modules(&self, root: &Path) -> Result<()> {
        let modules_path = match &self.modules_path {
            Some(p) => p,
            None => return Ok(()),
        };

        let source = Path::new(modules_path);
        if !source.exists() {
            eprintln!("Warning: modules path does not exist: {}", modules_path);
            return Ok(());
        }

        let dest = if let Some(ref version) = self.kernel_version {
            root.join("lib").join("modules").join(version)
        } else {
            root.join("lib").join("modules")
        };

        fs::create_dir_all(&dest)?;

        if source.is_file() {
            let file_name = source.file_name().unwrap();
            fs::copy(source, dest.join(file_name))?;
            println!("Copied module: {:?}", dest.join(file_name));
        } else {
            for entry in WalkDir::new(source).into_iter().filter_map(|e| e.ok()) {
                let entry_path = entry.path();

                // Use symlink_metadata to check the actual file type (not following symlinks)
                let meta = match entry_path.symlink_metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Warning: cannot read metadata for {:?}: {}", entry_path, e);
                        continue;
                    }
                };

                // Skip broken symlinks and other special files
                if meta.file_type().is_symlink() {
                    match entry_path.read_link() {
                        Ok(target) => {
                            if !target.exists() {
                                eprintln!("Warning: skipping broken symlink: {:?}", entry_path);
                                continue;
                            }
                        }
                        Err(_) => {
                            eprintln!("Warning: skipping broken symlink: {:?}", entry_path);
                            continue;
                        }
                    }
                }

                // Only process regular files and directories
                if !meta.is_dir() && !meta.is_file() {
                    eprintln!("Warning: skipping special file: {:?}", entry_path);
                    continue;
                }

                let relative = entry_path.strip_prefix(source).unwrap();
                let dest_path = dest.join(relative);

                if meta.is_dir() {
                    fs::create_dir_all(&dest_path)?;
                } else {
                    if let Some(parent) = dest_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    if let Err(e) = fs::copy(entry_path, &dest_path) {
                        eprintln!("Warning: failed to copy {:?}: {}", entry_path, e);
                        continue;
                    }
                }
            }
            println!("Copied modules to {:?}", dest);
        }

        Ok(())
    }

    fn copy_firmware(&self, root: &Path) -> Result<()> {
        let firmware_path = match &self.firmware_path {
            Some(p) => p,
            None => return Ok(()),
        };

        let source = Path::new(firmware_path);
        if !source.exists() {
            eprintln!("Warning: firmware path does not exist: {}", firmware_path);
            return Ok(());
        }

        let dest = root.join("lib").join("firmware");
        fs::create_dir_all(&dest)?;

        if source.is_file() {
            let file_name = source.file_name().unwrap();
            fs::copy(source, dest.join(file_name))?;
            println!("Copied firmware: {:?}", dest.join(file_name));
        } else {
            for entry in WalkDir::new(source).into_iter().filter_map(|e| e.ok()) {
                let entry_path = entry.path();

                // Use symlink_metadata to check the actual file type (not following symlinks)
                let meta = match entry_path.symlink_metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Warning: cannot read metadata for {:?}: {}", entry_path, e);
                        continue;
                    }
                };

                // Skip broken symlinks and other special files
                if meta.file_type().is_symlink() {
                    match entry_path.read_link() {
                        Ok(target) => {
                            if !target.exists() {
                                eprintln!("Warning: skipping broken symlink: {:?}", entry_path);
                                continue;
                            }
                        }
                        Err(_) => {
                            eprintln!("Warning: skipping broken symlink: {:?}", entry_path);
                            continue;
                        }
                    }
                }

                // Only process regular files and directories
                if !meta.is_dir() && !meta.is_file() {
                    eprintln!("Warning: skipping special file: {:?}", entry_path);
                    continue;
                }

                let relative = entry_path.strip_prefix(source).unwrap();
                let dest_path = dest.join(relative);

                if meta.is_dir() {
                    fs::create_dir_all(&dest_path)?;
                } else {
                    if let Some(parent) = dest_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    if let Err(e) = fs::copy(entry_path, &dest_path) {
                        eprintln!("Warning: failed to copy {:?}: {}", entry_path, e);
                        continue;
                    }
                }
            }
            println!("Copied firmware to {:?}", dest);
        }

        Ok(())
    }

    fn copy_extra_files(&self, root: &Path) -> Result<()> {
        for file_spec in &self.extra_files {
            let parts: Vec<&str> = file_spec.splitn(2, ':').collect();
            if parts.len() != 2 {
                eprintln!(
                    "Warning: invalid file spec '{}', expected src:dst",
                    file_spec
                );
                continue;
            }

            let src = Path::new(parts[0]);
            let dst_spec = parts[1];

            if !src.exists() {
                eprintln!("Warning: source file does not exist: {:?}", src);
                continue;
            }

            let dst = root.join(dst_spec.trim_start_matches('/'));

            let dest_path = if dst_spec.ends_with('/') {
                dst
            } else if dst.exists() && dst.is_dir() {
                dst
            } else {
                dst.clone()
            };

            let final_dest = if dest_path.is_dir() || dst_spec.ends_with('/') {
                let file_name = src.file_name().unwrap_or_default();
                dest_path.join(file_name)
            } else {
                dest_path
            };

            if let Some(parent) = final_dest.parent() {
                fs::create_dir_all(parent)?;
            }

            fs::copy(src, &final_dest)?;
            println!("Copied extra file: {:?} -> {:?}", src, final_dest);
        }

        Ok(())
    }

    fn create_init_link(&self, root: &Path) -> Result<()> {
        let init_link = root.join("init");
        std::os::unix::fs::symlink("bin/chivebox", &init_link)?;
        println!("Created init symlink -> bin/chivebox");
        Ok(())
    }

    fn package(&self, root: &Path) -> Result<()> {
        println!("Packaging initramfs...");

        let cpio_path = self.output_path.with_extension("cpio");

        // Use find | cpio to create the archive
        let status = std::process::Command::new("sh")
            .args(["-c", "find . -print | cpio -o --format=newc"])
            .current_dir(root)
            .stdout(File::create(&cpio_path)?)
            .status()?;

        if !status.success() {
            return Err(crate::error::Error::BuildFailure("cpio failed".to_string()));
        }

        // Compress with gzip
        let cpio_file = File::open(&cpio_path)?;
        let output_file = File::create(&self.output_path)?;
        let mut encoder =
            flate2::write::GzEncoder::new(output_file, flate2::Compression::default());
        let mut reader = BufReader::new(cpio_file);
        std::io::copy(&mut reader, &mut encoder)?;

        fs::remove_file(&cpio_path)?;

        println!("Created initramfs: {:?}", self.output_path);
        Ok(())
    }
}
