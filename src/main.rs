//! SdvxOnEar entry point: parse args, plan the conversion tasks, and run them through the packager.

use music_db::load_index;
use packer::{ensure_ffmpeg, package};
use scan::{build_special_tasks, filter_existing, scan_music_dir};

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
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

    /// number of parallel workers [default: logical CPU count]
    #[arg(short, long)]
    jobs: Option<usize>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let jobs = cli.jobs.unwrap_or_else(|| thread::available_parallelism().map_or(1, |n| n.get())).max(1);

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
