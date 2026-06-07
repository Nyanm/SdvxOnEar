//! SdvxOnEar entry point: parse args, plan the conversion tasks, and run them through the packager.

use common::MusicInfo;
use music_db::load_index;
use packer::{ensure_ffmpeg, package};
use scan::{build_special_tasks, filter_existing, scan_music_dir};
use tool::dump_music_csv;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use anyhow::{Context, Result, bail};
use clap::Parser;

mod common;
mod music_db;
mod packer;
mod scan;
mod tool;

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

    /// number of parallel workers [default: logical CPU count]
    #[arg(short, long)]
    jobs: Option<usize>,

    /// dump the parsed database to ./music_db.csv and exit (for auditing decode/fixup issues)
    #[arg(long)]
    csv: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let jobs = cli.jobs.unwrap_or_else(|| thread::available_parallelism().map_or(1, |n| n.get())).max(1);

    // load the base database from <src>/data/others/music_db.xml
    let path_xml = cli.src.join("data").join("others").join("music_db.xml");
    let mut vec_music = load_index(&path_xml)?;

    // fold in an omnimix patch if installed (revived deleted songs); searched up to 2 levels under the contents dir
    let path_omni = find_omnimix(&cli.src);
    if let Some(omni) = &path_omni {
        let vec_omni = load_index(&omni.join("others").join("music_db.merged.xml"))?;
        let count_omni = merge_omnimix(&mut vec_music, vec_omni);
        println!("omnimix: +{count_omni} revived songs from {}", omni.display());
    }
    println!("db: {} valid songs", vec_music.iter().filter(|m| m.is_valid).count());

    // --csv: dump the parsed db to ./music_db.csv for manual decode auditing, then exit (no conversion)
    if cli.csv {
        let path_csv = std::env::current_dir().context("resolving current directory")?.join("music_db.csv");
        dump_music_csv(&vec_music, &path_csv)?;
        println!("wrote {}", path_csv.display());
        return Ok(());
    }

    // resolve the conversion inputs/output (cwd output if -d omitted), then verify ffmpeg is runnable
    let path_music = cli.src.join("data").join("music");
    let path_music_omni = path_omni.as_ref().map(|o| o.join("music"));  // omnimix songs' audio root, if patched
    let path_out = match cli.dst {
        Some(path) => path,
        None => std::env::current_dir().context("resolving current directory for output")?,
    };
    if !path_music.is_dir() {
        bail!("music folder not found: {} (is --src pointing at the SDVX 'contents' directory?)", path_music.display());
    }
    ensure_ffmpeg()?;

    // plan tasks (standard scan + multi-audio specials), then unless --force drop those already converted
    let mut vec_task = scan_music_dir(&path_music, path_music_omni.as_deref(), &path_out, &vec_music);
    vec_task.extend(build_special_tasks(&path_music, &path_out, &vec_music));
    let count_planned = vec_task.len();
    if !cli.force {
        filter_existing(&mut vec_task);
    }
    let count_total = vec_task.len();
    println!("converting {count_total} tracks with {jobs} workers ({} already present) -> {}", count_planned - count_total, path_out.display());

    /*
    Fixed pool of `jobs` workers. Each worker atomically claims the next task index via fetch_add and processes it
    until the list is exhausted, so the load self-balances regardless of per-song duration. Scoped threads borrow
    vec_task and the counters by reference (no Arc); the atomics need no locking. Per-song failures are logged and
    skipped, never aborting the batch.
    */
    let idx_next = AtomicUsize::new(0);                                 // next task index to hand out
    let count_done = AtomicUsize::new(0);                               // finished (ok or fail), for progress + final count
    let count_fail = AtomicUsize::new(0);
    thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| loop {
                let idx = idx_next.fetch_add(1, Ordering::Relaxed);
                if idx >= count_total {
                    break;
                }
                let task = &vec_task[idx];
                if let Err(e) = package(&task.info, &task.music_path, &task.jacket, &task.dst_path) {
                    count_fail.fetch_add(1, Ordering::Relaxed);
                    eprintln!("FAIL #{} {}: {e:#}", task.info.id, task.info.str_title);
                }
                let count = count_done.fetch_add(1, Ordering::Relaxed) + 1;
                if count % 100 == 0 || count == count_total {
                    println!("  {count}/{count_total}");
                }
            });
        }
    });

    let count_fail = count_fail.load(Ordering::Relaxed);
    println!("done: {} converted, {count_fail} failed", count_total - count_fail);
    Ok(())
}

// --- omnimix patch helpers ----------------------------------------------------------------------------------------

// locate an "omnimix" directory within `path_root`, searching up to 2 levels deep (the patch sits at e.g.
// contents/data_mods/omnimix). Returns the shallowest match, or None when no patch is installed.
fn find_omnimix(path_root: &Path) -> Option<PathBuf> {
    find_dir(path_root, "omnimix", 2)
}

// depth-bounded search for a sub-directory with the given name (checks the current level before recursing)
fn find_dir(path_dir: &Path, str_name: &str, depth: u32) -> Option<PathBuf> {
    let mut vec_sub = Vec::new();
    for entry in fs::read_dir(path_dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some(str_name) {
                return Some(path);
            }
            vec_sub.push(path);
        }
    }
    if depth == 0 {
        return None;
    }
    vec_sub.into_iter().find_map(|p| find_dir(&p, str_name, depth - 1))
}

// fold an omnimix index into the base one: each valid omnimix song fills a gap in `vec_base` (revived deleted songs
// reuse their original ids) and is flagged is_omnimix so the scanner looks under the omnimix music dir. Returns how
// many were added; existing valid base entries are never overwritten.
fn merge_omnimix(vec_base: &mut Vec<MusicInfo>, vec_omni: Vec<MusicInfo>) -> usize {
    let mut count_added = 0;
    for mut info in vec_omni {
        if !info.is_valid {
            continue;
        }
        let idx = info.id as usize;
        if idx >= vec_base.len() {
            vec_base.resize(idx + 1, MusicInfo::default());
        }
        if !vec_base[idx].is_valid {
            info.is_omnimix = true;
            vec_base[idx] = info;
            count_added += 1;
        }
    }
    count_added
}
