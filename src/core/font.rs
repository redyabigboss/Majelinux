use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::core::{Error, Result};

const RENAME_IDS: [u16; 8] = [0, 1, 2, 3, 4, 6, 13, 14];

const COPYRIGHT: &str = "Emoji artwork: Twemoji, Copyright Twitter, Inc and other contributors, CC-BY 4.0. Font: Mozilla twemoji-colr, Apache 2.0. Modified: family name changed.";
const LICENSE: &str = "Emoji artwork licensed under CC-BY 4.0; font code under Apache License 2.0.";
const LICENSE_URL: &str = "https://creativecommons.org/licenses/by/4.0/";

fn names_for(id: u16) -> Option<&'static str> {
    match id {
        0 => Some(COPYRIGHT),
        1 => Some("Segoe UI Emoji"),
        2 => Some("Regular"),
        3 => Some("Local rename of Twemoji Mozilla 0.7.0"),
        4 => Some("Segoe UI Emoji"),
        6 => Some("SegoeUIEmoji"),
        13 => Some(LICENSE),
        14 => Some(LICENSE_URL),
        _ => None,
    }
}

#[derive(Clone)]
struct Record {
    platform: u16,
    encoding: u16,
    language: u16,
    name_id: u16,
    bytes: Vec<u8>,
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    let slice = data
        .get(offset..offset + 2)
        .ok_or_else(|| Error("truncated font table".into()))?;
    Ok(u16::from_be_bytes([slice[0], slice[1]]))
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    let slice = data
        .get(offset..offset + 4)
        .ok_or_else(|| Error("truncated font table".into()))?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn utf16be(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_be_bytes());
    }
    out
}

fn mac_bytes(text: &str) -> Vec<u8> {
    text.bytes().collect()
}

fn parse_name_table(data: &[u8]) -> Result<Vec<Record>> {
    let count = u16_at(data, 2)? as usize;
    let storage = u16_at(data, 4)? as usize;
    let mut records = Vec::with_capacity(count);
    for index in 0..count {
        let base = 6 + index * 12;
        let platform = u16_at(data, base)?;
        let encoding = u16_at(data, base + 2)?;
        let language = u16_at(data, base + 4)?;
        let name_id = u16_at(data, base + 6)?;
        let length = u16_at(data, base + 8)? as usize;
        let offset = u16_at(data, base + 10)? as usize;
        let start = storage
            .checked_add(offset)
            .ok_or_else(|| Error("bad name offset".into()))?;
        let bytes = data
            .get(start..start + length)
            .ok_or_else(|| Error("name string outside the table".into()))?
            .to_vec();
        records.push(Record {
            platform,
            encoding,
            language,
            name_id,
            bytes,
        });
    }
    Ok(records)
}

fn build_name_table(source: &[u8]) -> Result<Vec<u8>> {
    let mut records = parse_name_table(source)?;
    records.retain(|record| !RENAME_IDS.contains(&record.name_id));
    for id in RENAME_IDS {
        let Some(text) = names_for(id) else {
            continue;
        };
        records.push(Record {
            platform: 3,
            encoding: 1,
            language: 0x409,
            name_id: id,
            bytes: utf16be(text),
        });
        records.push(Record {
            platform: 1,
            encoding: 0,
            language: 0,
            name_id: id,
            bytes: mac_bytes(text),
        });
    }
    records.sort_by_key(|record| {
        (
            record.platform,
            record.encoding,
            record.language,
            record.name_id,
        )
    });

    let count = records.len();
    let storage_start = 6 + count * 12;
    let mut storage: Vec<u8> = Vec::new();
    let mut seen: HashMap<Vec<u8>, (u16, u16)> = HashMap::new();
    let mut offsets: Vec<(u16, u16)> = Vec::with_capacity(count);
    for record in &records {
        if let Some(found) = seen.get(&record.bytes) {
            offsets.push(*found);
            continue;
        }
        let offset: u16 = storage
            .len()
            .try_into()
            .map_err(|_| Error("name table is too large".into()))?;
        let length: u16 = record
            .bytes
            .len()
            .try_into()
            .map_err(|_| Error("name string is too long".into()))?;
        storage.extend_from_slice(&record.bytes);
        seen.insert(record.bytes.clone(), (offset, length));
        offsets.push((offset, length));
    }
    let total = storage_start + storage.len();
    if total > u16::MAX as usize {
        return Err(Error("name table is too large".into()));
    }

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&(count as u16).to_be_bytes());
    out.extend_from_slice(&(storage_start as u16).to_be_bytes());
    for (record, (offset, length)) in records.iter().zip(offsets.iter()) {
        for value in [
            record.platform,
            record.encoding,
            record.language,
            record.name_id,
            *length,
            *offset,
        ] {
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
    out.extend_from_slice(&storage);
    Ok(out)
}

fn checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let mut index = 0;
    while index + 4 <= data.len() {
        sum = sum.wrapping_add(u32::from_be_bytes([
            data[index],
            data[index + 1],
            data[index + 2],
            data[index + 3],
        ]));
        index += 4;
    }
    if index < data.len() {
        let mut tail = [0u8; 4];
        tail[..data.len() - index].copy_from_slice(&data[index..]);
        sum = sum.wrapping_add(u32::from_be_bytes(tail));
    }
    sum
}

struct Table {
    tag: [u8; 4],
    data: Vec<u8>,
    offset: usize,
    checksum: u32,
}

pub fn rename_bytes(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 12 {
        return Err(Error("font file is too small".into()));
    }
    let version = &data[..4];
    let count = u16_at(data, 4)? as usize;
    if count == 0 || 12 + count * 16 > data.len() {
        return Err(Error("invalid sfnt directory".into()));
    }
    let mut tables: Vec<Table> = Vec::with_capacity(count);
    let mut name_seen = false;
    for index in 0..count {
        let base = 12 + index * 16;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&data[base..base + 4]);
        let offset = u32_at(data, base + 8)? as usize;
        let length = u32_at(data, base + 12)? as usize;
        let slice = data
            .get(offset..offset + length)
            .ok_or_else(|| Error("font table outside the file".into()))?;
        let table_data = if &tag == b"name" {
            name_seen = true;
            build_name_table(slice)?
        } else {
            slice.to_vec()
        };
        tables.push(Table {
            tag,
            data: table_data,
            offset,
            checksum: 0,
        });
    }
    if !name_seen {
        return Err(Error("font has no name table".into()));
    }

    tables.sort_by_key(|table| table.offset);
    let directory_size = 12 + count * 16;
    let mut cursor = directory_size;
    for table in &mut tables {
        table.offset = cursor;
        let mut checksum_input = table.data.clone();
        if &table.tag == b"head" && checksum_input.len() >= 12 {
            checksum_input[8..12].fill(0);
        }
        table.checksum = checksum(&checksum_input);
        cursor += (table.data.len() + 3) & !3;
    }
    let total_size = cursor;

    let mut out = vec![0u8; total_size];
    out[..4].copy_from_slice(version);
    out[4..6].copy_from_slice(&(count as u16).to_be_bytes());
    let (search_range, entry_selector, range_shift) = search_range(count as u16);
    out[6..8].copy_from_slice(&search_range.to_be_bytes());
    out[8..10].copy_from_slice(&entry_selector.to_be_bytes());
    out[10..12].copy_from_slice(&range_shift.to_be_bytes());

    let mut directory: Vec<&Table> = tables.iter().collect();
    directory.sort_by_key(|a| a.tag);
    for (index, table) in directory.iter().enumerate() {
        let base = 12 + index * 16;
        out[base..base + 4].copy_from_slice(&table.tag);
        out[base + 4..base + 8].copy_from_slice(&table.checksum.to_be_bytes());
        out[base + 8..base + 12].copy_from_slice(&(table.offset as u32).to_be_bytes());
        out[base + 12..base + 16].copy_from_slice(&(table.data.len() as u32).to_be_bytes());
    }

    let mut head_offset = None;
    for table in &tables {
        out[table.offset..table.offset + table.data.len()].copy_from_slice(&table.data);
        if &table.tag == b"head" && table.data.len() >= 12 {
            head_offset = Some(table.offset);
        }
    }
    let head_offset = head_offset.ok_or_else(|| Error("font has no head table".into()))?;
    out[head_offset + 8..head_offset + 12].fill(0);
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
    out[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    Ok(out)
}

fn search_range(count: u16) -> (u16, u16, u16) {
    let mut power = 1u16;
    let mut selector = 0u16;
    while power * 2 <= count {
        power *= 2;
        selector += 1;
    }
    let range = power * 16;
    (range, selector, count * 16 - range)
}

pub fn rename_font(src: &Path, dst: &Path) -> Result<()> {
    let data = fs::read(src).map_err(|error| Error(tf_error(&error.to_string())))?;
    let renamed = rename_bytes(&data)?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dst.with_extension("majelinux.tmp");
    fs::write(&tmp, &renamed)?;
    if let Err(error) = fs::rename(&tmp, dst) {
        let _ = fs::remove_file(&tmp);
        return Err(error.into());
    }
    Ok(())
}

fn tf_error(detail: &str) -> String {
    crate::i18n::tf("err.font_rename", &[("err", detail)])
}

#[cfg(test)]
pub fn family_name(data: &[u8]) -> Result<String> {
    let count = u16_at(data, 4)? as usize;
    for index in 0..count {
        let base = 12 + index * 16;
        if &data[base..base + 4] != b"name" {
            continue;
        }
        let offset = u32_at(data, base + 8)? as usize;
        let length = u32_at(data, base + 12)? as usize;
        let table = &data[offset..offset + length];
        for record in parse_name_table(table)? {
            if record.name_id == 1 && record.platform == 3 && record.encoding == 1 {
                let units: Vec<u16> = record
                    .bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_be_bytes(*pair))
                    .collect();
                return Ok(String::from_utf16_lossy(&units));
            }
        }
    }
    Err(Error("missing font family name".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16be(value: u16) -> [u8; 2] {
        value.to_be_bytes()
    }
    fn u32be(value: u32) -> [u8; 4] {
        value.to_be_bytes()
    }

    fn tiny_font() -> Vec<u8> {
        let mut name = Vec::new();
        let family = utf16be("Twemoji Mozilla");
        name.extend_from_slice(&u16be(0));
        name.extend_from_slice(&u16be(1));
        name.extend_from_slice(&u16be(18));
        for value in [3u16, 1, 0x409, 1, family.len() as u16, 0] {
            name.extend_from_slice(&u16be(value));
        }
        name.extend_from_slice(&family);

        let mut head = vec![0u8; 54];
        head[..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());

        let mut out = vec![0u8; 12 + 2 * 16];
        out[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        out[4..6].copy_from_slice(&u16be(2));
        let (search, selector, shift) = search_range(2);
        out[6..8].copy_from_slice(&u16be(search));
        out[8..10].copy_from_slice(&u16be(selector));
        out[10..12].copy_from_slice(&u16be(shift));

        let head_offset = out.len();
        out.extend_from_slice(&head);
        let name_offset = out.len();
        out.extend_from_slice(&name);

        for (index, (tag, offset, length)) in [
            (b"head", head_offset, head.len()),
            (b"name", name_offset, name.len()),
        ]
        .iter()
        .enumerate()
        {
            let base = 12 + index * 16;
            out[base..base + 4].copy_from_slice(*tag);
            out[base + 4..base + 8].copy_from_slice(&u32be(0));
            out[base + 8..base + 12].copy_from_slice(&u32be(*offset as u32));
            out[base + 12..base + 16].copy_from_slice(&u32be(*length as u32));
        }
        out
    }

    #[test]
    fn renames_family_on_both_platforms() {
        let font = tiny_font();
        let renamed = rename_bytes(&font).unwrap();
        assert_eq!(family_name(&renamed).unwrap(), "Segoe UI Emoji");
        let count = u16_at(&renamed, 4).unwrap() as usize;
        let mut found = Vec::new();
        for index in 0..count {
            let base = 12 + index * 16;
            if &renamed[base..base + 4] == b"name" {
                let offset = u32_at(&renamed, base + 8).unwrap() as usize;
                let length = u32_at(&renamed, base + 12).unwrap() as usize;
                for record in parse_name_table(&renamed[offset..offset + length]).unwrap() {
                    found.push((record.platform, record.name_id));
                }
            }
        }
        assert!(found.contains(&(1, 1)));
        assert!(found.contains(&(3, 1)));
        assert!(found.contains(&(1, 4)));
        assert!(found.contains(&(3, 6)));
    }

    #[test]
    fn recomputes_master_checksum() {
        let font = tiny_font();
        let renamed = rename_bytes(&font).unwrap();
        assert_eq!(checksum(&renamed), 0xB1B0_AFBA);
    }

    #[test]
    fn rewrites_all_family_records() {
        let font = tiny_font();
        let renamed = rename_bytes(&font).unwrap();
        let text = String::from_utf8_lossy(&renamed);
        assert_eq!(family_name(&renamed).unwrap(), "Segoe UI Emoji");
        assert!(text.contains("Local rename of Twemoji Mozilla 0.7.0"));
        assert!(!text.contains("\u{0}Twemoji Mozilla"));
    }

    #[test]
    fn writes_atomically() {
        let dir = std::env::temp_dir().join(format!(
            "majelinux-font-{}-{}",
            std::process::id(),
            crate::core::util::unique_suffix()
        ));
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.ttf");
        let dst = dir.join("dst.ttf");
        fs::write(&src, tiny_font()).unwrap();
        rename_font(&src, &dst).unwrap();
        let data = fs::read(&dst).unwrap();
        assert_eq!(family_name(&data).unwrap(), "Segoe UI Emoji");
        let _ = fs::remove_dir_all(&dir);
    }
}
