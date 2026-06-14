//! Package one task into a tagged Opus file: convert the source audio to Ogg/Opus via iidx_on_knitting,
//! then attach the Vorbis tags + cover with lofty.
//!
//! The source is a pre-mixed lossy file (`.s3v` = ASF/WMA Pro, or an omnimix `.2dx`); `iidx_on_knitting::convert_song`
//! decodes it and re-encodes Ogg/Opus (it owns the vendored libav + libopus, so SdvxOnEar links no ffmpeg directly).
//! Source is already lossy, so Opus (not FLAC) is the target. lofty then writes the Vorbis comments and embeds the
//! cover art into the resulting `.opus` file.

use crate::common::{ALBUM_ARTIST, MusicInfo, version_album_name};

use std::fs;
use std::path::Path;
use anyhow::{Context, Result};

use iidx_on_knitting::convert_song;

use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::tag::{Tag, TagType};

// convert the source audio to opus via iidx_on_knitting, then attach Vorbis tags + the embedded cover
pub fn package(info: &MusicInfo, music_path: &Path, jacket: &Path, dst_path: &Path) -> Result<()> {
    if let Some(path_parent) = dst_path.parent() {
        fs::create_dir_all(path_parent).with_context(|| format!("creating output dir {}", path_parent.display()))?;
    }

    // do the work on a temp file in the same folder, then atomically rename into place, so an interrupted run never
    // leaves a half-written `.opus` that the incremental scan would mistake for done. Temp keeps the `.opus` suffix so
    // lofty still detects the format by extension; rename replaces any existing dst (MoveFileEx on Windows).
    let path_temp = dst_path.with_extension("part.opus");
    if let Err(e) = convert_song(music_path, &path_temp)
        .map_err(anyhow::Error::new)                                    // RenderError -> anyhow
        .and_then(|()| write_tags(info, jacket, &path_temp))
    {
        let _ = fs::remove_file(&path_temp);                            // best-effort cleanup of the partial temp
        return Err(e);
    }
    fs::rename(&path_temp, dst_path).with_context(|| format!("finalizing {}", dst_path.display()))?;
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
