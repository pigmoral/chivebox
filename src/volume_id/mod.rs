use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

const EXT_SUPER_MAGIC: u16 = 0xEF53;
const EXT_SUPERBLOCK_OFFSET: u64 = 0x400;

const XFS_MAGIC: &[u8; 4] = b"XFSB";
const BTRFS_MAGIC: &[u8; 8] = b"_BHRfS_M";
const F2FS_MAGIC: u32 = 0xF2F52010;
const SQUASHFS_MAGIC: &[u8; 4] = b"hsqs";

#[derive(Debug, Clone)]
pub struct FsInfo {
    pub fs_type: String,
    pub label: String,
    pub uuid: String,
}

pub fn probe_device(path: &str) -> Option<FsInfo> {
    let mut file = File::open(path).ok()?;
    probe_all(&mut file)
}

fn probe_all(file: &mut File) -> Option<FsInfo> {
    probe_ext(file)
        .or_else(|| probe_xfs(file))
        .or_else(|| probe_btrfs(file))
        .or_else(|| probe_f2fs(file))
        .or_else(|| probe_squashfs(file))
        .or_else(|| probe_vfat(file))
        .or_else(|| probe_ntfs(file))
}

fn read_at(file: &mut File, offset: u64, len: usize) -> Option<Vec<u8>> {
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn probe_ext(file: &mut File) -> Option<FsInfo> {
    let buf = read_at(file, EXT_SUPERBLOCK_OFFSET, 0x400)?;
    if buf.len() < 0x400 {
        return None;
    }

    let magic = u16::from_le_bytes([buf[0x38], buf[0x39]]);
    if magic != EXT_SUPER_MAGIC {
        return None;
    }

    let label: String = buf[0x78..0x88]
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as char)
        .collect();

    let uuid_bytes = &buf[0x68..0x78];
    let uuid = format_uuid(uuid_bytes);

    let feature_incompat = u32::from_le_bytes([buf[0x60], buf[0x61], buf[0x62], buf[0x63]]);
    let feature_ro_compat = u32::from_le_bytes([buf[0x64], buf[0x65], buf[0x66], buf[0x67]]);
    let has_journal = u32::from_le_bytes([buf[0x5c], buf[0x5d], buf[0x5e], buf[0x5f]]) != 0;

    let fs_type = if (feature_ro_compat & 0x08 != 0)
        || (feature_incompat & 0x02 != 0)
        || (feature_incompat & 0x80 != 0)
    {
        "ext4"
    } else if has_journal {
        "ext3"
    } else {
        "ext2"
    };

    Some(FsInfo {
        fs_type: fs_type.to_string(),
        label,
        uuid,
    })
}

fn probe_xfs(file: &mut File) -> Option<FsInfo> {
    let buf = read_at(file, 0, 0x200)?;
    if &buf[0..4] != XFS_MAGIC {
        return None;
    }

    let label: String = buf[0x6c..0x78]
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as char)
        .collect();

    let uuid_bytes = &buf[0x20..0x30];
    let uuid = format_uuid(uuid_bytes);

    Some(FsInfo {
        fs_type: "xfs".to_string(),
        label,
        uuid,
    })
}

fn probe_btrfs(file: &mut File) -> Option<FsInfo> {
    let buf = read_at(file, 0x10000, 0x1000)?;
    if &buf[0x40..0x48] != BTRFS_MAGIC {
        return None;
    }

    let label: String = buf[0x12b..0x22b]
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as char)
        .collect();

    let uuid_bytes = &buf[0x20..0x30];
    let uuid = format_uuid(uuid_bytes);

    Some(FsInfo {
        fs_type: "btrfs".to_string(),
        label,
        uuid,
    })
}

fn probe_f2fs(file: &mut File) -> Option<FsInfo> {
    let buf = read_at(file, 0x400, 0x200)?;
    if buf.len() < 0x4 {
        return None;
    }

    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != F2FS_MAGIC {
        return None;
    }

    let uuid_bytes = &buf[0x88..0x98];
    let uuid = format_uuid(uuid_bytes);

    Some(FsInfo {
        fs_type: "f2fs".to_string(),
        label: String::new(),
        uuid,
    })
}

fn probe_squashfs(file: &mut File) -> Option<FsInfo> {
    let buf = read_at(file, 0, 0x20)?;
    if &buf[0..4] != SQUASHFS_MAGIC {
        return None;
    }

    Some(FsInfo {
        fs_type: "squashfs".to_string(),
        label: String::new(),
        uuid: String::new(),
    })
}

fn probe_vfat(file: &mut File) -> Option<FsInfo> {
    let buf = read_at(file, 0, 0x200)?;
    if buf.len() < 0x200 {
        return None;
    }

    let magic = &buf[0x52..0x5a];
    if magic != b"FAT32   " && magic != b"FAT16   " && magic != b"FAT12   " {
        let magic2 = &buf[0x36..0x3e];
        if magic2 != b"FAT16   " && magic2 != b"FAT12   " {
            return None;
        }
    }

    let label: String = buf[0x43..0x4e]
        .iter()
        .map(|&c| if c == 0 { ' ' } else { c as char })
        .collect();

    let vol_id = u32::from_le_bytes([buf[0x27], buf[0x28], buf[0x29], buf[0x2a]]);
    let uuid = format!("{:08X}", vol_id);

    Some(FsInfo {
        fs_type: "vfat".to_string(),
        label: label.trim().to_string(),
        uuid,
    })
}

fn probe_ntfs(file: &mut File) -> Option<FsInfo> {
    let buf = read_at(file, 0, 0x200)?;
    if buf.len() < 0x10 {
        return None;
    }

    if &buf[0x3..0x7] != b"NTFS" {
        return None;
    }

    let uuid_bytes = &buf[0x48..0x50];
    let uuid = format_uuid(uuid_bytes);

    Some(FsInfo {
        fs_type: "ntfs".to_string(),
        label: String::new(),
        uuid,
    })
}

fn format_uuid(bytes: &[u8]) -> String {
    if bytes.len() < 16 {
        return String::new();
    }
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}
