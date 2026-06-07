//! Load `music_db.xml` (SHIFT-JIS) into a dense, id-indexed `Vec<MusicInfo>`.
//!
//! Pipeline: read bytes -> SHIFT-JIS decode -> hardcoded character fixups -> roxmltree parse -> dense vector. The
//! vector index equals the song id; id gaps (deleted licensed songs) are filled with default (is_valid == false)
//! entries. Numeric fields are converted to display form here (bpm zeros trimmed, date as YYYY-MM-DD).

use crate::common::{MusicInfo, FIXUP_RULES};

use std::fs;
use std::path::Path;
use anyhow::{Context, Result};

use encoding_rs::SHIFT_JIS;
use roxmltree::{Document, Node};

// public entry: load and index the whole music database, returning a vector indexed by song id
pub fn load_index(path_xml: &Path) -> Result<Vec<MusicInfo>> {
    let str_xml = decode_xml(path_xml)?;
    parse_index(&str_xml)
}

// --- decode: SHIFT-JIS (CP932) bytes -> UTF-8 string + hardcoded fixups -------------------------------------------

// read the xml as SHIFT-JIS, decode to UTF-8, then apply the hardcoded fixup table
fn decode_xml(path_xml: &Path) -> Result<String> {
    let vec_bytes = fs::read(path_xml).with_context(|| format!("reading music_db xml {}", path_xml.display()))?;

    let (cow_text, _enc, flag_had_errors) = SHIFT_JIS.decode(&vec_bytes);
    let mut str_xml = cow_text.into_owned();
    if flag_had_errors {
        eprintln!("[music_db] warning: some bytes failed SHIFT-JIS decoding and became U+FFFD, consider adding fixups");
    }

    for &(str_from, str_to) in FIXUP_RULES {
        str_xml = str_xml.replace(str_from, str_to);
    }

    Ok(str_xml)
}

// --- parse: xml -> dense id-indexed vector ------------------------------------------------------------------------

// parse decoded xml into the dense id-indexed vector
fn parse_index(str_xml: &str) -> Result<Vec<MusicInfo>> {
    // we already hold UTF-8 text, so rewrite the shift-jis declaration to keep the document self-consistent
    let str_normalized = str_xml.replacen("encoding=\"shift-jis\"", "encoding=\"UTF-8\"", 1);
    let doc = Document::parse(&str_normalized).context("parsing music_db xml")?;
    let node_root = doc.root_element();                                 // <mdb>

    let mut vec_index: Vec<MusicInfo> = vec![MusicInfo::default()];     // slot 0 unused, ids start at 1
    let mut iter_music = node_root.children().filter(|n| n.has_tag_name("music"));
    let mut node_music = iter_music.next();
    let mut index: u32 = 1;

    while let Some(node) = node_music {
        let info = parse_music(index, node);
        index += 1;
        if info.is_valid {
            node_music = iter_music.next();                             // matched: consume this node
        }
        vec_index.push(info);
    }

    Ok(vec_index)
}

// compare the node's id against the expected dense `index`: on match read the fields (is_valid == true), otherwise
// return a default placeholder (is_valid == false) standing in for a deleted/missing id
fn parse_music(index: u32, node_music: Node) -> MusicInfo {
    if node_music.attribute("id").and_then(|s| s.parse::<u32>().ok()) != Some(index) {
        return MusicInfo::default();
    }

    // a matched node is always consumed (is_valid == true) even if <info> is somehow absent, to avoid stalling the merge
    let node_info = match node_music.children().find(|n| n.has_tag_name("info")) {
        Some(n) => n,
        None => return MusicInfo { is_valid: true, id: index, ..Default::default() },
    };

    MusicInfo {
        is_valid: true,
        is_omnimix: false,
        id: index,
        str_ascii: child_text(node_info, "ascii").to_string(),
        str_title: child_text(node_info, "title_name").to_string(),
        str_title_yomigana: child_text(node_info, "title_yomigana").to_string(),
        str_artist: child_text(node_info, "artist_name").to_string(),
        str_artist_yomigana: child_text(node_info, "artist_yomigana").to_string(),
        str_date: format_date(child_num(node_info, "distribution_date")),
        str_bpm: format_bpm(child_num(node_info, "bpm_max")),
        version: child_num(node_info, "version"),
    }
}

// text of the first child element with the given tag name, or "" if absent
fn child_text<'a>(node_parent: Node<'a, '_>, str_tag: &str) -> &'a str {
    node_parent
        .children()
        .find(|n| n.has_tag_name(str_tag))
        .and_then(|n| n.text())
        .unwrap_or("")                                                  // text() borrows from the document arena ('a)
}

// numeric child text parsed into T, falling back to T::default() on missing/garbage values
fn child_num<T: std::str::FromStr + Default>(node_parent: Node<'_, '_>, str_tag: &str) -> T {
    child_text(node_parent, str_tag).trim().parse::<T>().unwrap_or_default()
}

// raw YYYYMMDD integer (e.g. 20200702) -> "2020-07-02"; 0 (missing) -> empty string
fn format_date(ymd: u32) -> String {
    if ymd == 0 {
        return String::new();
    }
    format!("{:04}-{:02}-{:02}", ymd / 10000, ymd / 100 % 100, ymd % 100)
}

// raw bpm scaled by 100 (e.g. 18500 / 8250) -> trailing-zero-trimmed text ("185" / "82.5")
fn format_bpm(raw_x100: u32) -> String {
    let whole = raw_x100 / 100;
    let frac = raw_x100 % 100;
    if frac == 0 {
        format!("{whole}")
    } else if frac % 10 == 0 {
        format!("{whole}.{}", frac / 10)
    } else {
        format!("{whole}.{frac:02}")
    }
}
