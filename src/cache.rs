//! Bounded, versioned snapshot. Corruption is an ordinary cache miss, never an empty index.
use crate::{
    catalog::Catalog,
    model::{AppEntry, LaunchTarget},
};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

const MAGIC: &[u8; 8] = b"PRCA\x01\0\0\0";
const LIMIT: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid PicoRun cache")
}
fn hash(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
fn put(data: &mut Vec<u8>, bytes: &[u8]) {
    data.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    data.extend_from_slice(bytes);
}
fn take<'a>(data: &mut &'a [u8]) -> io::Result<&'a [u8]> {
    if data.len() < 4 {
        return Err(invalid());
    }
    let size = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
    *data = &data[4..];
    if size > data.len() {
        return Err(invalid());
    }
    let value = &data[..size];
    *data = &data[size..];
    Ok(value)
}
fn string(data: &mut &[u8]) -> io::Result<String> {
    String::from_utf8(take(data)?.to_vec()).map_err(|_| invalid())
}

#[cfg(windows)]
fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}
#[cfg(windows)]
fn decode_path(data: &[u8]) -> io::Result<PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    if !data.len().is_multiple_of(2) {
        return Err(invalid());
    }
    let wide: Vec<_> = data
        .chunks_exact(2)
        .map(|v| u16::from_le_bytes([v[0], v[1]]))
        .collect();
    if wide.contains(&0) {
        return Err(invalid());
    }
    Ok(PathBuf::from(OsString::from_wide(&wide)))
}
#[cfg(not(windows))]
fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().as_bytes().to_vec()
}
#[cfg(not(windows))]
fn decode_path(data: &[u8]) -> io::Result<PathBuf> {
    Ok(PathBuf::from(
        std::str::from_utf8(data).map_err(|_| invalid())?,
    ))
}

pub fn load(path: &Path) -> io::Result<Catalog> {
    if fs::metadata(path)?.len() > LIMIT as u64 {
        return Err(invalid());
    }
    // Bound the read even if another process changes the file after metadata().
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() < 20 || bytes.len() > LIMIT || &bytes[..8] != MAGIC {
        return Err(invalid());
    }
    let checksum = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    if hash(&bytes[16..]) != checksum {
        return Err(invalid());
    }
    let count = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    if count > MAX_ENTRIES {
        return Err(invalid());
    }
    let mut data = &bytes[20..];
    let mut entries = Vec::with_capacity(count.min(data.len() / 16));
    for _ in 0..count {
        let name = string(&mut data)?;
        let target = LaunchTarget::ShellPath(decode_path(take(&mut data)?)?);
        let key_count = *data.first().ok_or_else(invalid)? as usize;
        data = &data[1..];
        if name.is_empty() || key_count == 0 || key_count > 3 {
            return Err(invalid());
        }
        let mut keys = Vec::with_capacity(key_count);
        for _ in 0..key_count {
            keys.push(string(&mut data)?);
        }
        entries.push(AppEntry { name, target, keys });
    }
    if !data.is_empty() {
        return Err(invalid());
    }
    Ok(Catalog::new(entries))
}

pub fn save(path: &Path, catalog: &Catalog) -> io::Result<()> {
    if catalog.entries().len() > MAX_ENTRIES {
        return Err(invalid());
    }
    let mut bytes = Vec::from(*MAGIC);
    bytes.extend_from_slice(&[0; 8]);
    bytes.extend_from_slice(&(catalog.entries().len() as u32).to_le_bytes());
    for entry in catalog.entries() {
        let LaunchTarget::ShellPath(path) = &entry.target else {
            return Err(invalid());
        };
        put(&mut bytes, entry.name.as_bytes());
        put(&mut bytes, &path_bytes(path));
        bytes.push(entry.keys.len() as u8);
        for key in &entry.keys {
            put(&mut bytes, key.as_bytes());
        }
        if bytes.len() > LIMIT {
            return Err(invalid());
        }
    }
    let checksum = hash(&bytes[16..]);
    bytes[8..16].copy_from_slice(&checksum.to_le_bytes());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    #[cfg(windows)]
    crate::platform::windows::replace_file(&temporary, path)?;
    #[cfg(not(windows))]
    fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_replace_and_corruption() {
        let directory =
            std::env::temp_dir().join(format!("picorun-cache-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("apps.bin");
        let catalog = Catalog::new(vec![AppEntry::new(
            "微信 QQ",
            LaunchTarget::ShellPath(PathBuf::from("中文 空格/微信.lnk")),
        )]);
        save(&path, &catalog).unwrap();
        save(&path, &catalog).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.entries()[0].target, catalog.entries()[0].target);
        assert_eq!(loaded.entries()[0].keys, catalog.entries()[0].keys);
        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(load(&path).is_err());
        fs::write(&path, b"PRCA").unwrap();
        assert!(load(&path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
