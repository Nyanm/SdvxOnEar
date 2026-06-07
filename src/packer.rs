//! Package one task into a tagged Opus file: ffmpeg (WMA -> Opus transcode) -> lofty (Vorbis tags + cover).
//!
//! `.s3v` is an ASF container holding lossy WMA Pro audio, which has no pure-Rust decoder, so a bundled `ffmpeg.exe`
//! transcodes it to Opus. Source is already lossy, so Opus (not FLAC) is the target. lofty then writes the Vorbis
//! comments and embeds the cover art into the resulting `.opus` file.

use crate::common::{ALBUM_ARTIST, MusicInfo, version_album_name};

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Context, Result, bail};

use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::tag::{Tag, TagType};

// libopus target bitrate; the source is ~384kbps lossy WMA Pro, so 192k Opus is effectively transparent
const OPUS_BITRATE: &str = "192k";

// fail fast before a batch if ffmpeg can't be run, with a message pointing at the fix
pub fn ensure_ffmpeg() -> Result<()> {
    let path_ff = ffmpeg_path();
    Command::new(&path_ff)
        .arg("-version")
        .output()
        .with_context(|| format!("cannot run ffmpeg ({}); put ffmpeg.exe next to the program or on PATH", path_ff.display()))?;
    Ok(())
}

// transcode the s3v to opus with ffmpeg, then attach Vorbis tags + the embedded cover
pub fn package(info: &MusicInfo, music_path: &Path, jacket: &Path, dst_path: &Path) -> Result<()> {
    if let Some(path_parent) = dst_path.parent() {
        fs::create_dir_all(path_parent).with_context(|| format!("creating output dir {}", path_parent.display()))?;
    }

    // do the work on a temp file in the same folder, then atomically rename into place, so an interrupted run never
    // leaves a half-written `.opus` that the incremental scan would mistake for done. Temp keeps the `.opus` suffix so
    // ffmpeg/lofty still detect the format by extension; rename replaces any existing dst (MoveFileEx on Windows).
    let path_temp = dst_path.with_extension("part.opus");
    if let Err(e) = transcode_opus(music_path, &path_temp).and_then(|()| write_tags(info, jacket, &path_temp)) {
        let _ = fs::remove_file(&path_temp);                            // best-effort cleanup of the partial temp
        return Err(e);
    }
    fs::rename(&path_temp, dst_path).with_context(|| format!("finalizing {}", dst_path.display()))?;
    Ok(())
}

// run ffmpeg to decode WMA and encode Opus; source metadata is dropped here, lofty writes our own afterwards
fn transcode_opus(music_path: &Path, dst_path: &Path) -> Result<()> {
    let status = Command::new(ffmpeg_path())
        .args(["-loglevel", "error", "-y", "-i"])
        .arg(music_path)
        .args(["-vn", "-map_metadata", "-1", "-c:a", "libopus", "-b:a", OPUS_BITRATE])
        .arg(dst_path)
        .status()
        .with_context(|| format!("spawning ffmpeg for {}", music_path.display()))?;
    if !status.success() {
        bail!("ffmpeg failed ({status}) converting {}", music_path.display());
    }
    Ok(())
}

// lofty: attach Vorbis comments + the cover picture to the encoded opus
fn write_tags(info: &MusicInfo, jacket: &Path, dst_path: &Path) -> Result<()> {
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title(info.str_title.clone());                              // TITLE
    tag.set_artist(info.str_artist.clone());                            // ARTIST
    tag.set_album(version_album_name(info.version).to_string());        // ALBUM (game version name)
    tag.insert_text(ItemKey::AlbumArtist, ALBUM_ARTIST.to_string());    // ALBUMARTIST (fixed "BEMANI" for grouping)
    tag.insert_text(ItemKey::TrackNumber, info.id.to_string());         // TRACKNUMBER (music id)
    insert_if_set(&mut tag, ItemKey::TrackTitleSortOrder, &info.str_title_yomigana);   // TITLESORT
    insert_if_set(&mut tag, ItemKey::TrackArtistSortOrder, &info.str_artist_yomigana); // ARTISTSORT
    insert_if_set(&mut tag, ItemKey::RecordingDate, &info.str_date);    // DATE
    insert_if_set(&mut tag, ItemKey::Bpm, &info.str_bpm);               // BPM

    let vec_jacket = fs::read(jacket).with_context(|| format!("reading jacket {}", jacket.display()))?;
    let picture = Picture::unchecked(vec_jacket)
        .pic_type(PictureType::CoverFront)
        .mime_type(MimeType::Png)
        .build();
    tag.push_picture(picture);

    tag.save_to_path(dst_path, WriteOptions::default())
        .with_context(|| format!("writing tags to {}", dst_path.display()))?;
    Ok(())
}

// insert a text field only when non-empty, to avoid writing blank Vorbis comments
fn insert_if_set(tag: &mut Tag, item_key: ItemKey, str_value: &str) {
    if !str_value.is_empty() {
        tag.insert_text(item_key, str_value.to_string());
    }
}

// locate ffmpeg.exe: prefer one next to our own executable (release layout), then the working dir, else fall back to
// PATH. Paths must be absolute: Command on Windows searches PATH, not the cwd, for a bare relative program name.
fn ffmpeg_path() -> PathBuf {
    if let Ok(path_exe) = std::env::current_exe() {
        if let Some(path_dir) = path_exe.parent() {
            let path_ff = path_dir.join("ffmpeg.exe");
            if path_ff.exists() {
                return path_ff;
            }
        }
    }
    if let Ok(path_cwd) = std::env::current_dir() {
        let path_ff = path_cwd.join("ffmpeg.exe");
        if path_ff.exists() {
            return path_ff;
        }
    }
    PathBuf::from("ffmpeg")
}
