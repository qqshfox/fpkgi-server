use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use log::{debug, error};

use crate::enums::{DRMCategory, ContentCategory, IROCategory};
use crate::utils::{read_u16_be, read_u32_be, read_u64_be, extract_string};

#[derive(Debug)]
pub struct PS4Package {
    pub filepath: PathBuf,
    pub file_entries: HashMap<u32, FileEntry>,
    pub content_id: String,
    pub content_type: ContentCategory,
    pub iro_type: Option<IROCategory>,
    pub drm_type: DRMCategory,
    pub hashes: Vec<String>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct FileEntry {
    pub name_pos: u32,
    pub flag1: u32,
    pub flag2: u32,
    pub offset: u64,
    pub size: u64,
    pub key_index: u32,
    pub encrypted: bool,
    pub name: Option<String>,
}

impl PS4Package {
    const VALID_MAGIC: u32 = 0x7F434E54;
    const HASH_POS: u64 = 0x0100;
    pub const FILE_POS: u32 = 0x0200;
    const HEADER_SIZE: usize = 416;
    const ENTRY_SIZE: usize = 32;

    pub fn new(filepath: PathBuf) -> Result<Self> {
        let mut pkg = PS4Package {
            filepath,
            file_entries: HashMap::new(),
            content_id: String::new(),
            content_type: ContentCategory::Game,
            iro_type: None,
            drm_type: DRMCategory::None,
            hashes: Vec::new(),
        };
        pkg.parse_package()?;
        Ok(pkg)
    }

    fn parse_package(&mut self) -> Result<()> {
        let mut file = File::open(&self.filepath)?;
        let file_size = file.metadata()?.len();
        debug!("PKG file size: {} bytes", file_size);

        if file_size < Self::HEADER_SIZE as u64 {
            error!("PKG file too small for header: {} bytes < {} bytes", file_size, Self::HEADER_SIZE);
            return Err(anyhow::anyhow!("PKG file too small for header"));
        }

        let mut header = vec![0u8; Self::HEADER_SIZE];
        debug!("Reading PKG header at offset 0 (size: {} bytes)", Self::HEADER_SIZE);
        file.read_exact(&mut header)?;

        let mut cursor = std::io::Cursor::new(&header);
        let magic = read_u32_be(&mut cursor)?;
        if magic != Self::VALID_MAGIC {
            error!("Invalid PKG magic value: {:08x}", magic);
            return Err(anyhow::anyhow!("Invalid PKG magic value"));
        }

        let pkg_type = read_u32_be(&mut cursor)?;
        let _unk1 = read_u32_be(&mut cursor)?;
        let file_count = read_u32_be(&mut cursor)?;
        let entry_count = read_u32_be(&mut cursor)?;
        let sc_entry_count = read_u16_be(&mut cursor)?;
        let _unk2 = read_u16_be(&mut cursor)?;
        let table_pos = read_u32_be(&mut cursor)? as u64;
        let entry_data_size = read_u32_be(&mut cursor)? as u64;
        let body_pos = read_u64_be(&mut cursor)?;
        let body_size = read_u64_be(&mut cursor)?;
        let content_pos = read_u64_be(&mut cursor)?;
        let content_size = read_u64_be(&mut cursor)?;
        let mut content_id = [0u8; 36];
        cursor.read_exact(&mut content_id)?;
        self.content_id = String::from_utf8_lossy(&content_id).trim_end_matches('\x00').to_string();
        let mut padding = [0u8; 12];
        cursor.read_exact(&mut padding)?;
        let drm_type = read_u32_be(&mut cursor)?;
        let content_type = read_u32_be(&mut cursor)?;
        let _content_flags = read_u32_be(&mut cursor)?;
        let _promote_size = read_u32_be(&mut cursor)?;
        let _version_date = read_u32_be(&mut cursor)?;
        let _version_hash = read_u32_be(&mut cursor)?;
        cursor.seek(SeekFrom::Current(32))?;
        let iro_type = read_u32_be(&mut cursor)?;
        let _drm_version = read_u32_be(&mut cursor)?;

        debug!("PKG Header - Magic: {:08x}", magic);
        debug!("PKG Type: {:08x}, File Count: {}, Entry Count: {}", pkg_type, file_count, entry_count);
        debug!("SC Entry Count: {}, Table Pos: {}, Entry Data Size: {}", sc_entry_count, table_pos, entry_data_size);
        debug!("ID: {}", self.content_id);
        debug!("DRM Type: {:08x}, Content Type: {:08x}", drm_type, content_type);
        debug!("Body Pos: {}, Body Size: {}, Content Pos: {}, Content Size: {}",
               body_pos, body_size, content_pos, content_size);

        self.drm_type = match drm_type {
            0x0 => DRMCategory::None,
            0xF => DRMCategory::PS4,
            _ => DRMCategory::None,
        };
        self.content_type = match content_type {
            0x1A => ContentCategory::Game,
            0x1B => ContentCategory::DLC,
            0x1C => ContentCategory::App,
            0x1E => ContentCategory::Demo,
            _ => ContentCategory::Game,
        };
        self.iro_type = match iro_type {
            0x1 => Some(IROCategory::SFTheme),
            0x2 => Some(IROCategory::SysTheme),
            _ => None,
        };

        if file_size < Self::HASH_POS + 128 {
            error!("PKG file too small for hash data: {} bytes < {} bytes", file_size, Self::HASH_POS + 128);
            return Err(anyhow::anyhow!("PKG file too small for hash data"));
        }

        debug!("Reading hash data at offset {} (size: 128 bytes)", Self::HASH_POS);
        debug!("Current file position before seek: {}", file.stream_position()?);
        file.seek(SeekFrom::Start(Self::HASH_POS))?;
        debug!("File position after seek to {}: {}", Self::HASH_POS, file.stream_position()?);
        let mut hash_data = [0u8; 128];
        file.read_exact(&mut hash_data)?;
        self.hashes = (0..64).step_by(16).map(|i| hex::encode(&hash_data[i..i+16])).collect();

        for (i, hash) in self.hashes.iter().enumerate() {
            debug!("Hash {}: {}", i + 1, hash);
        }
        debug!("Current file position after hash read: {}", file.stream_position()?);

        self.parse_files(&mut file, table_pos, entry_count as usize, entry_data_size)?;
        Ok(())
    }

    fn parse_files(&mut self, file: &mut File, table_pos: u64, entry_count: usize, entry_data_size: u64) -> Result<()> {
        let file_size = file.metadata()?.len();
        let expected_end = table_pos + (entry_count as u64 * Self::ENTRY_SIZE as u64);
        if file_size < expected_end {
            error!("PKG file too small for {} entries: {} bytes < {} bytes",
                   entry_count, file_size, expected_end);
            return Err(anyhow::anyhow!("PKG file too small for entry table"));
        }

        debug!("Reading {} PKG entries at offset {} (size: {} bytes)",
               entry_count, table_pos, entry_count * Self::ENTRY_SIZE);
        debug!("Current file position before seek: {}", file.stream_position()?);
        file.seek(SeekFrom::Start(table_pos))?;
        debug!("File position after seek to {}: {}", table_pos, file.stream_position()?);

        let mut raw_data = vec![0u8; 64.min(file_size.saturating_sub(table_pos) as usize)];
        if !raw_data.is_empty() {
            file.seek(SeekFrom::Start(table_pos))?;
            if let Ok(_) = file.read_exact(&mut raw_data) {
                debug!("Raw data at table_pos {} (first {} bytes): {}", table_pos, raw_data.len(), hex::encode(&raw_data));
            }
            file.seek(SeekFrom::Start(table_pos))?;
        }

        for i in 0..entry_count {
            let mut entry = vec![0u8; Self::ENTRY_SIZE];
            debug!("Reading entry {} at offset {}", i, table_pos + (i * Self::ENTRY_SIZE) as u64);
            debug!("Current file position before read: {}", file.stream_position()?);
            let bytes_read = file.read(&mut entry)?;
            if bytes_read < Self::ENTRY_SIZE {
                error!("Short read for entry {} at offset {}: read {} bytes, expected {}",
                       i, table_pos + (i * Self::ENTRY_SIZE) as u64, bytes_read, Self::ENTRY_SIZE);
                debug!("Partial entry data: {}", hex::encode(&entry[..bytes_read]));
                continue;
            }

            let mut cursor = std::io::Cursor::new(&entry);
            let entry_id = read_u32_be(&mut cursor)?;
            let name_pos = read_u32_be(&mut cursor)?;
            let flag1 = read_u32_be(&mut cursor)?;
            let flag2 = read_u32_be(&mut cursor)?;
            let offset = read_u32_be(&mut cursor)? as u64;
            let size = read_u32_be(&mut cursor)? as u64;
            let _padding = read_u64_be(&mut cursor)?;

            debug!("Entry {} - ID: {:08x}, Name Pos: {}, Offset: {}, Size: {}",
                   i, entry_id, name_pos, offset, size);
            debug!("Raw entry data: {}", hex::encode(&entry));

            self.file_entries.insert(entry_id, FileEntry {
                name_pos,
                flag1,
                flag2,
                offset,
                size,
                key_index: (flag2 & 0xF00) >> 12,
                encrypted: flag1 & 0x80000000 != 0,
                name: None,
            });
        }

        if self.file_entries.is_empty() {
            error!("No valid entries parsed from entry table");
            return Err(anyhow::anyhow!("No valid entries parsed"));
        }

        let file_pos = self.file_entries.get(&Self::FILE_POS).ok_or_else(|| {
            error!("Missing file table entry at ID {:08x}", Self::FILE_POS);
            anyhow::anyhow!("Missing file table entry")
        })?;

        if file_size < file_pos.offset + entry_data_size {
            error!("PKG file too small for name buffer: {} bytes < {} bytes",
                   file_size, file_pos.offset + entry_data_size);
            return Err(anyhow::anyhow!("PKG file too small for name buffer"));
        }

        debug!("Reading name buffer at offset {} (size: {} bytes)", file_pos.offset, entry_data_size);
        debug!("Current file position before name buffer read: {}", file.stream_position()?);
        file.seek(SeekFrom::Start(file_pos.offset))?;
        let mut name_buffer = vec![0u8; entry_data_size as usize];
        file.read_exact(&mut name_buffer)?;

        for (entry_id, entry) in self.file_entries.iter_mut() {
            if entry.name_pos as usize >= name_buffer.len() {
                error!("Name offset out of bounds for entry {:08x}: {} >= {}",
                       entry_id, entry.name_pos, name_buffer.len());
                continue;
            }
            let name = extract_string(&name_buffer, entry.name_pos as usize);
            if !name.is_empty() {
                entry.name = Some(name.clone());
                let enc_status = if entry.encrypted { "ENCRYPTED" } else { "UNENCRYPTED" };
                debug!("Entry {:08x}: {} ({} bytes, offset {:08x}, {})",
                       entry_id, name, entry.size, entry.offset, enc_status);
            }
        }
        Ok(())
    }

    pub fn get_file(&self, identifier: &str) -> Result<Vec<u8>> {
        let file_data = self.locate_file(identifier)?;
        let mut file = File::open(&self.filepath)?;
        let file_size = file.metadata()?.len();

        if file_data.offset + file_data.size > file_size {
            error!("File data out of bounds: offset {} + size {} > file size {}",
                   file_data.offset, file_data.size, file_size);
            return Err(anyhow::anyhow!("File data out of bounds"));
        }

        debug!("Reading file data for '{}': offset {}, size {}", identifier, file_data.offset, file_data.size);
        file.seek(SeekFrom::Start(file_data.offset))?;
        let mut buffer = vec![0u8; file_data.size as usize];
        file.read_exact(&mut buffer)?;
        Ok(buffer)
    }

    pub fn save_file(&self, identifier: &str, destination: &Path) -> Result<()> {
        let data = self.get_file(identifier)?;
        let mut output = File::create(destination)?;
        output.write_all(&data)?;
        debug!("Saved file '{}' to '{}'", identifier, destination.display());
        Ok(())
    }

    fn locate_file(&self, identifier: &str) -> Result<&FileEntry> {
        if let Ok(entry_id) = u32::from_str_radix(identifier.trim_start_matches("0x"), 16) {
            self.file_entries.get(&entry_id).ok_or_else(|| anyhow::anyhow!("File not found: {}", identifier))
        } else {
            self.file_entries.values()
                .find(|entry| entry.name.as_ref().map_or(false, |n| n == identifier))
                .ok_or_else(|| anyhow::anyhow!("File not found: {}", identifier))
        }
    }
}
