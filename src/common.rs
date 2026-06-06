//! Shared data definitions used across modules: the parsed song record, the version table, and the packaging tasks.

use std::path::PathBuf;

// one song entry; index in the loaded vector equals `id`, gaps are left with `is_valid == false`
#[derive(Default, Clone, Debug)]
pub struct MusicInfo {
    pub is_valid: bool,                 // false for id gaps and placeholder entries, skipped on traversal
    pub id: u32,                        // music id, also the vector index and the 4-digit folder/file prefix
    pub str_ascii: String,             // ascii name, the on-disk prefix linking the db entry to its folder
    pub str_title: String,             // -> TITLE
    pub str_title_yomigana: String,    // -> TITLE SORT, kana reading
    pub str_artist: String,            // -> ARTIST
    pub str_artist_yomigana: String,   // -> ARTIST, kana reading
    pub str_date: String,              // distribution date as YYYY-MM-DD -> DATE
    pub str_bpm: String,               // bpm_max with trailing zeros trimmed, e.g. "185" or "82.5" -> BPM
    pub version: u8,                    // game version 1..=6, used to pick the output sub-folder
    pub inf_ver: u8,                    // 4th-difficulty class: 0=none 2=INF 3=GRV 4=HVN 5=VVD 6=XCD, for remix naming
}

// one FLAC to produce: a single audio source plus its resolved cover, info and destination. `info` is owned so a remix
// variant can carry its own title. The packager is later called as package(&info, &music_path, &jacket, &dst_path).
#[derive(Debug)]
pub struct PackTask {
    pub info: MusicInfo,                // song metadata, title already adjusted for remixes (unchanged for standard songs)
    pub music_path: PathBuf,            // the `.s3v` (renamed wav) to decode
    pub jacket: PathBuf,                // the cover `_b.png` to embed
    pub dst_path: PathBuf,              // output `.flac` path
}

// one hand-specified output for a multi-audio special: audio source token ("" = base `.s3v`, else `_<token>.s3v`),
// preferred jacket slot (fallback applies), and title ("" = keep the db title, else a full override)
pub struct SpecialTask {
    pub id: u32,
    pub audio_token: &'static str,
    pub jacket_slot: u8,
    pub title: &'static str,
}

// folder name per game version; index == version number, slot 0 unused, slot 7 reserved for a future title
pub const VERSION_FOLDER_NAMES: [&str; 8] = [
    "",
    "01 BOOTH",
    "02 II -infinite infection-",
    "03 III GRAVITY WARS",
    "04 IV HEAVENLY HAVEN",
    "05 V VIVID WAVE",
    "06 EXCEED GEAR",
    "07 ∇",
];

// album tag per game version (the clean full game name); index == version number, slot 0 unused
pub const VERSION_ALBUM_NAMES: [&str; 8] = [
    "",
    "SOUND VOLTEX BOOTH",
    "SOUND VOLTEX II -infinite infection-",
    "SOUND VOLTEX III GRAVITY WARS",
    "SOUND VOLTEX IV HEAVENLY HAVEN",
    "SOUND VOLTEX V VIVID WAVE",
    "SOUND VOLTEX EXCEED GEAR",
    "SOUND VOLTEX ∇",
];

// fixed ALBUMARTIST tag so each version-album groups correctly despite differing per-track artists
pub const ALBUM_ARTIST: &str = "BEMANI";

// multi-audio songs handled by SPECIAL_TASKS; the general scan skips these ids so it can stay simple
pub const SPECIAL_IDS: &[u32] = &[26, 709, 822, 927, 1148, 1225, 1758];

// explicit per-output rows for the multi-audio specials; shared fields (artist/bpm/date/version) come from the db by id
pub const SPECIAL_TASKS: &[SpecialTask] = &[
    SpecialTask { id: 26,   audio_token: "",   jacket_slot: 3, title: "" },
    SpecialTask { id: 26,   audio_token: "4i", jacket_slot: 4, title: "ごりらがいるんだ - Infinity Edit - " },
    SpecialTask { id: 709,  audio_token: "",   jacket_slot: 3, title: "" },
    SpecialTask { id: 709,  audio_token: "4i", jacket_slot: 4, title: "月光乱舞 - Gravity Edit - " },
    SpecialTask { id: 822,  audio_token: "",   jacket_slot: 1, title: "" },
    SpecialTask { id: 822,  audio_token: "4i", jacket_slot: 4, title: "Help me, ERINNNNNN!! - SH Style Gravity Edit -" },
    SpecialTask { id: 927,  audio_token: "",   jacket_slot: 1, title: "" },
    SpecialTask { id: 927,  audio_token: "4i", jacket_slot: 4, title: "極圏 - Heavenly Edit - " },
    SpecialTask { id: 1148, audio_token: "1n", jacket_slot: 1, title: "TWO-TORIAL - Novice Edit - " },
    SpecialTask { id: 1148, audio_token: "2a", jacket_slot: 2, title: "TWO-TORIAL - Advance Edit - " },
    SpecialTask { id: 1148, audio_token: "3e", jacket_slot: 3, title: "TWO-TORIAL - Exhaust Edit - " },
    SpecialTask { id: 1148, audio_token: "5m", jacket_slot: 5, title: "TWO-TORIAL - Maximum Edit - " },
    SpecialTask { id: 1225, audio_token: "",   jacket_slot: 1, title: "" },
    SpecialTask { id: 1225, audio_token: "5m", jacket_slot: 1, title: "Evans - Maximum Edit - " },
    SpecialTask { id: 1758, audio_token: "1n", jacket_slot: 1, title: "Help me, ERINNNNNN!! #幻想郷ホロイズムver. - Pekora Usada, Miko Sakura, Shion Murasaki Edit - " },
    SpecialTask { id: 1758, audio_token: "2a", jacket_slot: 2, title: "Help me, ERINNNNNN!! #幻想郷ホロイズムver. - Marine Houshou, Fubuki Shirakami, Rushia Uruha Edit - " },
    SpecialTask { id: 1758, audio_token: "3e", jacket_slot: 3, title: "Help me, ERINNNNNN!! #幻想郷ホロイズムver. - Marine Houshou, Matsuri Natsuiro, Aqua Minato Edit - " },
    SpecialTask { id: 1758, audio_token: "5m", jacket_slot: 5, title: "Help me, ERINNNNNN!! #幻想郷ホロイズムver. - Noel Shirogane, Flare Shiranui Edit - " },
];

// safe accessor: out-of-range versions fall back to an empty string instead of panicking
pub fn version_folder_name(version: u8) -> &'static str {
    VERSION_FOLDER_NAMES.get(version as usize).copied().unwrap_or("")
}

// album name for the version's ALBUM tag; out-of-range falls back to an empty string
pub fn version_album_name(version: u8) -> &'static str {
    VERSION_ALBUM_NAMES.get(version as usize).copied().unwrap_or("")
}
