//! Anvil region-file arithmetic.
//!
//! A region file starts with two 4 KiB tables: chunk locations then timestamps. A location entry
//! packs a 3-byte sector offset and a 1-byte sector count, and every chunk payload is padded to a
//! whole number of 4096-byte sectors. None of that may change: existing worlds must stay readable
//! by vanilla, by Forge and by every external tool.
//!
//! These helpers exist so the sector bookkeeping can be validated in isolation, away from the
//! `RandomAccessFile` it normally hides behind, and so the same rules can later back a native
//! region writer.

/// Size of one sector, fixed by the Anvil format.
pub const SECTOR_BYTES: usize = 4096;
/// Number of chunks addressed by one region file (32x32).
pub const CHUNKS_PER_REGION: usize = 1024;
/// Sectors reserved for the location and timestamp tables.
pub const HEADER_SECTORS: u32 = 2;
/// Largest payload a location entry can describe: 255 sectors, i.e. just under 1 MiB.
pub const MAX_INLINE_SECTORS: u32 = 255;

/// Number of whole sectors needed to hold `length` bytes of payload.
pub fn sectors_for(length: usize) -> u32 {
    ((length + SECTOR_BYTES - 1) / SECTOR_BYTES) as u32
}

/// Packs a location table entry. Returns `None` when the chunk does not fit the format.
pub fn encode_location(sector_offset: u32, sector_count: u32) -> Option<u32> {
    if sector_offset > 0x00FF_FFFF || sector_count > MAX_INLINE_SECTORS {
        return None;
    }
    Some((sector_offset << 8) | sector_count)
}

/// Unpacks a location table entry into (sector offset, sector count).
pub fn decode_location(entry: u32) -> (u32, u32) {
    (entry >> 8, entry & 0xFF)
}

/// Index of a chunk inside the location table, from its coordinates.
pub fn location_index(chunk_x: i32, chunk_z: i32) -> usize {
    let x = (chunk_x & 31) as usize;
    let z = (chunk_z & 31) as usize;
    x + z * 32
}

/// Checks that a location table describes a file that can be read without overlap.
///
/// Returns the list of problems found rather than the first one: a region file with several
/// damaged entries should be reported once, not repaired one restart at a time.
pub fn validate_locations(entries: &[u32], file_sectors: u32) -> Vec<String> {
    let mut problems = Vec::new();
    let mut occupancy: Vec<Option<usize>> = vec![None; file_sectors.max(HEADER_SECTORS) as usize];

    for (index, &entry) in entries.iter().enumerate() {
        if entry == 0 {
            continue; // chunk not present, which is normal
        }
        let (offset, count) = decode_location(entry);
        if count == 0 {
            problems.push(format!("chunk {index}: zero sector count"));
            continue;
        }
        if offset < HEADER_SECTORS {
            problems.push(format!("chunk {index}: payload overlaps the header at sector {offset}"));
            continue;
        }
        if offset as usize + count as usize > occupancy.len() {
            problems.push(format!(
                "chunk {index}: payload runs past the end of the file (sector {offset}, {count} sectors)"
            ));
            continue;
        }
        for sector in offset..offset + count {
            match occupancy[sector as usize] {
                Some(other) => problems.push(format!(
                    "chunk {index}: sector {sector} already owned by chunk {other}"
                )),
                None => occupancy[sector as usize] = Some(index),
            }
        }
    }
    problems
}

/// Finds the first run of `count` free sectors, or the end of the file when there is none.
///
/// `used` marks sectors that are taken, header included. Allocating at the end is always valid,
/// so this never fails; it only prefers a hole when one is big enough.
pub fn find_free_run(used: &[bool], count: u32) -> u32 {
    if count == 0 {
        return used.len() as u32;
    }
    let mut run_start = None;
    for (index, &taken) in used.iter().enumerate() {
        if taken {
            run_start = None;
            continue;
        }
        let start = *run_start.get_or_insert(index);
        if index + 1 - start >= count as usize {
            return start as u32;
        }
    }
    used.len() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_rounding_matches_the_format() {
        assert_eq!(sectors_for(0), 0);
        assert_eq!(sectors_for(1), 1);
        assert_eq!(sectors_for(SECTOR_BYTES), 1);
        assert_eq!(sectors_for(SECTOR_BYTES + 1), 2);
        assert_eq!(sectors_for(SECTOR_BYTES * 255), 255);
    }

    #[test]
    fn location_entries_round_trip() {
        let entry = encode_location(1234, 7).unwrap();
        assert_eq!(decode_location(entry), (1234, 7));
    }

    #[test]
    fn oversized_chunks_are_refused_rather_than_truncated() {
        assert!(encode_location(2, 256).is_none());
        assert!(encode_location(0x0100_0000, 1).is_none());
    }

    #[test]
    fn location_index_wraps_to_the_region() {
        assert_eq!(location_index(0, 0), 0);
        assert_eq!(location_index(31, 31), 1023);
        assert_eq!(location_index(32, 32), 0);
        assert_eq!(location_index(-1, -1), 1023);
    }

    #[test]
    fn validation_detects_overlap_and_truncation() {
        let mut entries = vec![0u32; CHUNKS_PER_REGION];
        entries[0] = encode_location(2, 2).unwrap();
        entries[1] = encode_location(3, 2).unwrap(); // overlaps chunk 0
        entries[2] = encode_location(100, 1).unwrap(); // past the end
        entries[3] = encode_location(0, 1).unwrap(); // inside the header

        let problems = validate_locations(&entries, 6);
        assert!(problems.iter().any(|p| p.contains("already owned")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("past the end")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("header")), "{problems:?}");
    }

    #[test]
    fn a_clean_region_reports_no_problem() {
        let mut entries = vec![0u32; CHUNKS_PER_REGION];
        entries[0] = encode_location(2, 1).unwrap();
        entries[5] = encode_location(3, 2).unwrap();
        assert!(validate_locations(&entries, 5).is_empty());
    }

    #[test]
    fn allocation_prefers_holes_then_falls_back_to_the_end() {
        //            header     hole        used        hole
        let used = [true, true, false, false, true, false];
        assert_eq!(find_free_run(&used, 2), 2);
        assert_eq!(find_free_run(&used, 1), 2);
        assert_eq!(find_free_run(&used, 3), 6); // no hole big enough, append
    }
}
