//! SdvxOnEar entry point: parse args, plan the conversion tasks, and run them through the packager.

use music_db::load_index;
use packer::{ensure_ffmpeg, package};
use scan::{build_special_tasks, filter_existing, scan_music_dir};

use std::path::PathBuf;
use anyhow::{Context, Result, bail};
use clap::Parser;

mod common;
mod music_db;
mod packer;
mod scan;

// command-line arguments
#[derive(Parser)]
#[command(name = "SdvxOnEar", version, about = "Convert SDVX .s3v audio into tagged Opus with cover art")]
struct Cli {
    /// SDVX `contents` directory (data/music and data/others/music_db.xml are appended automatically)
    #[arg(short, long)]
    src: PathBuf,

    /// output directory [default: current directory]
    #[arg(short, long)]
    dst: Option<PathBuf>,

    /// force a full run, re-converting even songs already present in the output
    #[arg(short, long)]
    force: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // derive the two inputs from the contents dir, and resolve the output dir (cwd if -d omitted)
    let path_xml = cli.src.join("data").join("others").join("music_db.xml");
    let path_music = cli.src.join("data").join("music");
    let path_out = match cli.dst {
        Some(path) => path,
        None => std::env::current_dir().context("resolving current directory for output")?,
    };
    if !path_music.is_dir() {
        bail!("music folder not found: {} (is --src pointing at the SDVX 'contents' directory?)", path_music.display());
    }
    ensure_ffmpeg()?;

    let vec_music = load_index(&path_xml)?;
    println!("db: {} valid songs", vec_music.iter().filter(|m| m.is_valid).count());

    // plan tasks (standard scan + multi-audio specials), then unless --force drop those already converted
    let mut vec_task = scan_music_dir(&path_music, &path_out, &vec_music);
    vec_task.extend(build_special_tasks(&path_music, &path_out, &vec_music));
    let count_planned = vec_task.len();
    if !cli.force {
        filter_existing(&mut vec_task);
    }
    let count_total = vec_task.len();
    println!("converting {count_total} tracks ({} already present) -> {}", count_planned - count_total, path_out.display());

    // convert each track; keep going on per-song failures (locked file, bad audio, ...) and report at the end
    let mut count_ok: usize = 0;
    let mut count_fail: usize = 0;
    for (idx, task) in vec_task.iter().enumerate() {
        match package(&task.info, &task.music_path, &task.jacket, &task.dst_path) {
            Ok(()) => count_ok += 1,
            Err(e) => {
                count_fail += 1;
                eprintln!("[{}/{count_total}] FAIL #{} {}: {e:#}", idx + 1, task.info.id, task.info.str_title);
            }
        }
        if (idx + 1) % 100 == 0 || idx + 1 == count_total {
            println!("  {}/{count_total}", idx + 1);
        }
    }

    println!("done: {count_ok} converted, {count_fail} failed");
    Ok(())
}
