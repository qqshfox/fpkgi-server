use std::collections::HashMap;
use std::io::{Seek, SeekFrom, Cursor};

use anyhow::Result;
use log::{debug, error};

use crate::utils::{read_u16_le, read_u32_le, extract_string};

#[derive(Debug)]
pub struct SFOProcessor;

impl SFOProcessor {
    const MAGIC_BYTES: &'static [u8] = b"\x00PSF";
    const HEADER_SIZE: usize = 20;
    const ENTRY_SIZE: usize = 16;

    pub fn new() -> Self {
        SFOProcessor
    }

    pub fn process(&self, buffer: Vec<u8>) -> Result<HashMap<String, String>> {
        debug!("SFO buffer size: {} bytes", buffer.len());

        if !buffer.starts_with(Self::MAGIC_BYTES) {
            error!("Invalid SFO file: Magic bytes missing");
            return Err(anyhow::anyhow!("Invalid SFO file: Magic bytes missing"));
        }

        if buffer.len() < Self::HEADER_SIZE {
            error!("SFO buffer too small for header: {} bytes < {} bytes", buffer.len(), Self::HEADER_SIZE);
            return Err(anyhow::anyhow!("SFO buffer too small for header"));
        }

        let mut cursor = Cursor::new(&buffer);
        cursor.seek(SeekFrom::Start(Self::MAGIC_BYTES.len() as u64))?;

        let version = read_u32_le(&mut cursor)?;
        let key_table_start = read_u32_le(&mut cursor)? as usize;
        let data_table_start = read_u32_le(&mut cursor)? as usize;
        let entry_count = read_u32_le(&mut cursor)? as usize;

        debug!("SFO Header - Version: {:08x}, Key Table Start: {}, Data Table Start: {}, Entries: {}",
               version, key_table_start, data_table_start, entry_count);

        let entry_table_pos = Self::HEADER_SIZE;
        if buffer.len() < entry_table_pos + entry_count * Self::ENTRY_SIZE {
            error!("SFO buffer too small for {} entries: {} bytes < {} bytes",
                   entry_count, buffer.len(), entry_table_pos + entry_count * Self::ENTRY_SIZE);
            return Err(anyhow::anyhow!("SFO buffer too small for entries"));
        }

        let mut entries = Vec::new();
        for i in 0..entry_count {
            let offset = entry_table_pos + i * Self::ENTRY_SIZE;
            debug!("Reading SFO entry {} at offset {}", i, offset);
            cursor.seek(SeekFrom::Start(offset as u64))?;
            let key_pos = read_u16_le(&mut cursor)? as usize;
            let data_type = read_u16_le(&mut cursor)?;
            let data_size = read_u32_le(&mut cursor)? as usize;
            let _max_size = read_u32_le(&mut cursor)?;
            let data_pos = read_u32_le(&mut cursor)? as usize;
            entries.push((key_pos, data_type, data_size, data_pos));
        }

        let mut output = HashMap::new();
        for (i, (key_pos, data_type, data_size, data_pos)) in entries.into_iter().enumerate() {
            if key_table_start + key_pos >= buffer.len() {
                error!("Entry {} key offset out of bounds: {} >= {}", i, key_table_start + key_pos, buffer.len());
                continue;
            }
            let key = extract_string(&buffer, key_table_start + key_pos);

            if data_table_start + data_pos + data_size > buffer.len() {
                error!("Entry {} data offset out of bounds: {} + {} > {}",
                       i, data_table_start + data_pos, data_size, buffer.len());
                continue;
            }
            let raw_value = &buffer[data_table_start + data_pos..data_table_start + data_pos + data_size];

            debug!("Entry {} - Key: {}, Type: {:04x}, Size: {}, Data Offset: {}",
                   i, key, data_type, data_size, data_table_start + data_pos);

            let value = match data_type {
                0x0204 => String::from_utf8_lossy(raw_value).trim_end_matches('\x00').to_string(),
                0x0404 => {
                    if raw_value.len() < 4 {
                        error!("Entry {} integer data too short: {} bytes", i, raw_value.len());
                        hex::encode(raw_value)
                    } else {
                        u32::from_le_bytes(raw_value.try_into()?).to_string()
                    }
                }
                _ => {
                    log::info!("Entry {} unknown format {:04x} for key '{}', using hex", i, data_type, key);
                    hex::encode(raw_value)
                }
            };
            output.insert(key, value);
        }
        Ok(output)
    }
}
