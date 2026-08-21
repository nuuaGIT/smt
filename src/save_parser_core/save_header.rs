//! SaveFileInfo: the uncompressed header at the top of the .sav file.
//! Owned values (the raw file buffer is not retained after decompression).

use crate::error::{perr, PResult};
use crate::reader::Cursor;

#[derive(Debug, Clone)]
pub struct SaveFileInfo {
    pub save_header_type: u32,
    pub save_version: u32,
    pub build_version: u32,
    pub save_name: String,
    pub map_name: String,
    pub map_options: String,
    pub session_name: String,
    pub play_duration_in_seconds: u32,
    pub save_date_time_in_ticks: u64,
    pub session_visibility: u8, // Python keeps this as a 1-byte bytes object
    pub editor_object_version: u32,
    pub mod_metadata: String,
    pub is_modded_save: bool,
    pub save_identifier: String,
    pub save_data_hash: [u64; 2],
    pub is_creative_mode_enabled: bool,
}

pub const TICKS_IN_SECOND: u64 = 10_000_000;
pub const EPOCH_1_TO_1970: u64 = 719_162 * 24 * 60 * 60;

/// COMPAT EXPERIMENT: first saveVersion carrying the 1.0 body layout -- header
/// flags, the per-level version field, and the 1.0 collectables tail (see
/// level.rs, which gates all three on this). Below it the file is an Update
/// 8-era save; 42 is the only such version the accept list above takes today.
///
/// Read paths honour the gates either way. WRITE paths do not: every record
/// the editor emits is 1.0-format, so it refuses to touch a save below this
/// (see editor::apply::plan_op) rather than produce a file the game can't
/// load. Widening the accept list means auditing those write paths first.
pub const FIRST_1_0_SAVE_VERSION: u32 = 46;

/// Returns (header, offset where the compressed body begins).
pub fn parse_save_file_info(data: &[u8]) -> PResult<(SaveFileInfo, usize)> {
    let mut c = Cursor::new(data, 0);
    let save_header_type = c.u32()?;
    // COMPAT EXPERIMENT: 13 = Update 8 header (no saveName field).
    if !matches!(save_header_type, 13 | 14) {
        return Err(perr!(
            "Unsupported save header version number {}.",
            save_header_type
        ));
    }
    let save_version = c.u32()?;
    // COMPAT EXPERIMENT: 42 = Update 8.
    if !matches!(save_version, 42 | 52 | 53 | 58 | 59 | 60) {
        return Err(perr!("Unsupported save version number {}.", save_version));
    }
    let build_version = c.u32()?;
    // saveName exists from header type 14 (1.0) on.
    let save_name = if save_header_type >= 14 {
        c.string_owned()?
    } else {
        String::new()
    };
    let map_name = c.string_owned()?;
    let map_options = c.string_owned()?;
    let session_name = c.string_owned()?;
    let play_duration_in_seconds = c.u32()?;
    let save_date_time_in_ticks = c.u64()?;
    let session_visibility = c.u8()?;
    let editor_object_version = if save_version >= 7 { c.u32()? } else { 0 };
    let mod_metadata = if save_version >= 8 {
        c.string_owned()?
    } else {
        String::new()
    };
    let is_modded_save = c.bool_u32("isModdedSave")?;
    let save_identifier = if save_version >= 10 {
        c.string_owned()?
    } else {
        String::new()
    };

    let mut save_data_hash = [0u64; 2];
    let mut is_creative_mode_enabled = false;
    if save_version >= 13 {
        c.confirm_u32(1)?; // isPartitionedWorld
        c.confirm_u32(1)?;
        save_data_hash[0] = c.u64()?;
        save_data_hash[1] = c.u64()?;
        is_creative_mode_enabled = c.bool_u32("SaveFileInfo.isCreativeModeEnabled")?;
    }

    Ok((
        SaveFileInfo {
            save_header_type,
            save_version,
            build_version,
            save_name,
            map_name,
            map_options,
            session_name,
            play_duration_in_seconds,
            save_date_time_in_ticks,
            session_visibility,
            editor_object_version,
            mod_metadata,
            is_modded_save,
            save_identifier,
            save_data_hash,
            is_creative_mode_enabled,
        },
        c.pos,
    ))
}
