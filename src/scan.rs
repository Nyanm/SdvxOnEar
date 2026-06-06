//! Plan one packaging task per output FLAC.
//!
//! The general path is id-driven: it walks the valid `MusicInfo` entries, locates each song's folder from its id +
//! ascii name, and takes the base `.s3v` plus the highest jacket available (tried slot 5 -> 4 -> 3 -> 1). The few
//! multi-audio specials are skipped here and produced from the `SPECIAL_TASKS` table instead. An incremental filter
//! runs before the scan to drop songs already present in the output.

use crate::common::{MusicInfo, PackTask, SPECIAL_IDS, SPECIAL_TASKS, version_folder_name};

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use anyhow::Result;

// --- incremental support: drop songs already present in the output ------------------------------------------------

// collect ids already converted in the output folder, so a re-run only processes new songs.
// TODO incremental: output files are named by title, so there is no id to recover yet; this returns an empty set
// (i.e. full conversion) until an id tag or id-prefixed naming is introduced.
pub fn scan_done_ids(_path_out: &Path) -> Result<HashSet<u32>> {
    Ok(HashSet::new())
}

// mark every valid song whose id is already done as invalid, so the scan and special builder both skip it
pub fn filter_done(vec_index: &mut [MusicInfo], set_done_id: &HashSet<u32>) {
    for info in vec_index.iter_mut() {
        if info.is_valid && set_done_id.contains(&info.id) {
            info.is_valid = false;
        }
    }
}

// --- general scan: one task per standard song ---------------------------------------------------------------------

// walk the valid songs, locate each folder by id + ascii, and plan a packaging task per standard song
pub fn scan_music_dir(path_music: &Path, path_out: &Path, vec_index: &[MusicInfo]) -> Vec<PackTask> {
    let mut vec_task = Vec::new();

    for info in vec_index.iter().filter(|m| m.is_valid) {
        if SPECIAL_IDS.contains(&info.id) {
            continue;                                                   // produced by build_special_tasks instead
        }
        let str_prefix = format!("{:04}_{}", info.id, info.str_ascii);  // folder/file name "<id4>_<ascii>"
        let path_dir = path_music.join(&str_prefix);
        if let Some(task) = resolve_song(info, &path_dir, &str_prefix, path_out) {
            vec_task.push(task);
        }
    }

    vec_task
}

// plan the task for one standard song: base audio + highest available jacket. None when the base audio is absent.
fn resolve_song(info: &MusicInfo, path_dir: &Path, str_prefix: &str, path_out: &Path) -> Option<PackTask> {
    let str_id4 = str_prefix.split('_').next().unwrap_or("");          // "0927", the jacket file prefix

    let path_base = path_dir.join(format!("{str_prefix}.s3v"));
    if !path_base.exists() {
        return None;                                                   // jacket-only db row (delisted/event), nothing to do
    }

    // jacket of the highest difficulty available: MAXIMUM -> 4th -> EXHAUST -> ADVANCED -> base
    let path_jacket = find_jacket(path_dir, str_id4, [5, 4, 3, 2, 1])?;

    Some(PackTask {
        info: info.clone(),
        music_path: path_base,
        jacket: path_jacket,
        dst_path: build_dst(path_out, info.version, &info.str_title),
    })
}

// --- special table: multi-audio songs, one explicit row per output FLAC -------------------------------------------

// build the multi-audio special tasks from SPECIAL_TASKS, pulling shared fields from the db entry by id
pub fn build_special_tasks(path_music: &Path, path_out: &Path, vec_index: &[MusicInfo]) -> Vec<PackTask> {
    let mut vec_task = Vec::new();
    for sp in SPECIAL_TASKS {
        let info_db = match vec_index.get(sp.id as usize) {
            Some(info) if info.is_valid => info,
            _ => continue,                                             // missing in db or already done (filtered out)
        };

        let str_id4 = format!("{:04}", sp.id);
        let str_prefix = format!("{str_id4}_{}", info_db.str_ascii);
        let path_dir = path_music.join(&str_prefix);

        let str_audio = if sp.audio_token.is_empty() {
            format!("{str_prefix}.s3v")
        } else {
            format!("{str_prefix}_{}.s3v", sp.audio_token)
        };
        // the specified slot, falling back to the nearest lower jacket that exists (ultimately slot 1)
        let path_jacket = match find_jacket(&path_dir, &str_id4, (1..=sp.jacket_slot).rev()) {
            Some(p) => p,
            None => {
                eprintln!("[special] no jacket for id {} (slot {})", sp.id, sp.jacket_slot);
                continue;
            }
        };

        let mut info = info_db.clone();
        if !sp.title.is_empty() {
            info.str_title = sp.title.to_string();
        }
        let path_dst = build_dst(path_out, info.version, &info.str_title);

        vec_task.push(PackTask {
            info,
            music_path: path_dir.join(str_audio),
            jacket: path_jacket,
            dst_path: path_dst,
        });
    }
    vec_task
}

// --- helpers ------------------------------------------------------------------------------------------------------

// first existing "jk_<id4>_<slot>_b.png" among the given slots, tried in order
fn find_jacket(path_dir: &Path, str_id4: &str, slots: impl IntoIterator<Item = u8>) -> Option<PathBuf> {
    slots
        .into_iter()
        .map(|slot| path_dir.join(format!("jk_{str_id4}_{slot}_b.png")))
        .find(|p| p.exists())
}

// destination path: <out>/<version folder>/<sanitized title>.opus
fn build_dst(path_out: &Path, version: u8, str_title: &str) -> PathBuf {
    path_out
        .join(version_folder_name(version))
        .join(format!("{}.opus", sanitize_filename(str_title)))
}

// replace characters illegal in Windows file names with '_'
fn sanitize_filename(str_title: &str) -> String {
    str_title
        .chars()
        .map(|c| if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect()
}
