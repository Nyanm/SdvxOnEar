//! SdvxOnEar entry point.

use music_db::load_index;
use packer::package;
use scan::{build_special_tasks, filter_done, scan_done_ids, scan_music_dir};

use std::fs;
use std::path::Path;
use anyhow::Result;

mod common;
mod music_db;
mod packer;
mod scan;

fn main() -> Result<()> {
    let path_xml = Path::new(r"");
    let path_music = Path::new(r"");
    let path_out = Path::new(r"");          // placeholder, real output dir comes with CLI args

    let mut vec_music = load_index(path_xml)?;

    // incremental filter: drop songs already present in the output (no-op until scan_done_ids is implemented)
    let set_done_id = scan_done_ids(path_out)?;
    filter_done(&mut vec_music, &set_done_id);
    println!("db: vec len = {}, valid = {}", vec_music.len(), vec_music.iter().filter(|m| m.is_valid).count());

    // pack music path info (music + jacket) into tasks
    let mut vec_task = scan_music_dir(path_music, path_out, &vec_music);
    let count_standard = vec_task.len();
    vec_task.extend(build_special_tasks(path_music, path_out, &vec_music));
    println!("scan: {} tasks ({} standard + {} special)", vec_task.len(), count_standard, vec_task.len() - count_standard);

    Ok(())
}
