//! Diagnostic helpers, kept out of the normal conversion path.
//!
//! `dump_music_csv` writes the parsed database to a CSV for manual auditing. The arcade font privately remaps some
//! CP932 kanji code points to symbols (e.g. byte pair `EA 99` decodes to 齷 but the game shows "é"), so no standard
//! decoder can recover the intended glyph — the CSV lets the user spot these and add fixup rules in music_db.

use crate::common::MusicInfo;

use std::fs;
use std::path::Path;
use anyhow::{Context, Result};

// write every valid song's fields (except is_valid) to a UTF-8 CSV with a BOM (so Excel reads CJK/symbols correctly)
pub fn dump_music_csv(vec_music: &[MusicInfo], path_csv: &Path) -> Result<()> {
    let mut str_out = String::from("\u{FEFF}");                         // UTF-8 BOM for Excel
    str_out.push_str("id,ascii,title,title_yomigana,artist,artist_yomigana,date,bpm,version\n");
    for info in vec_music.iter().filter(|m| m.is_valid) {
        str_out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            info.id,
            csv_field(&info.str_ascii),
            csv_field(&info.str_title),
            csv_field(&info.str_title_yomigana),
            csv_field(&info.str_artist),
            csv_field(&info.str_artist_yomigana),
            csv_field(&info.str_date),
            csv_field(&info.str_bpm),
            info.version,
        ));
    }
    fs::write(path_csv, str_out).with_context(|| format!("writing csv {}", path_csv.display()))?;
    Ok(())
}

// quote a field per RFC 4180 when it contains a comma, quote, or newline (titles/artists often have commas/parens)
fn csv_field(str_value: &str) -> String {
    if str_value.contains(|c| matches!(c, ',' | '"' | '\n' | '\r')) {
        format!("\"{}\"", str_value.replace('"', "\"\""))
    } else {
        str_value.to_string()
    }
}
