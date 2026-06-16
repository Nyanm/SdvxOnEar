# SDVX on Ear

## 简介

SDVX on Ear是一款用于将SDVX游戏文件中的歌曲提取为组织好的音乐文件仓库的程序，同时会为每一首歌加入对应的元信息与封面。
程序会尝试读取你的`contents/`文件夹，将游戏文件中的歌曲（包括omnimix文件夹）从游戏原生格式（`.s3v` 的 WMA Pro，以及早期 omni 的 `.2dx`）转换为Opus格式（SDVX原生格式对元信息标签的支持很差），再依据版本放入对应的文件夹中。
在游戏更新到新版本后，可以直接再次运行程序，程序将增量转换新增的歌曲。

每首歌附加的元信息包括：

| 元信息   | Vorbis 标签     | 来源                                   |
|-------|---------------|--------------------------------------|
| 曲名    | `TITLE`       | 游戏内曲名                                |
| 艺术家   | `ARTIST`      | 游戏内艺术家名                              |
| 专辑    | `ALBUM`       | 所属版本全名（如 `SOUND VOLTEX EXCEED GEAR`） |
| 专辑艺术家 | `ALBUMARTIST` | 固定为 `BEMANI`，保证同一版本归为一张专辑            |
| 音轨号   | `TRACKNUMBER` | 歌曲在游戏中的 id                           |
| 曲名排序  | `TITLESORT`   | 曲名读音（半角片假名）                          |
| 艺术家排序 | `ARTISTSORT`  | 艺术家读音                                |
| 发行日期  | `DATE`        | 形如 `2020-07-02`                      |
| BPM   | `BPM`         | 最大 BPM                               |
| 封面    | 内嵌图片          | 最高难度的封面（`jk_<id>_<难度>_b.png`）        |

本程序不包含任何所属©Konami Arcade Games版权所有的信息。

## 使用

### 编译

本程序通过 path 依赖嵌入 [IIDX on Knitting](https://github.com/Nyanm/iidxOnKnitting)。请将两个仓库克隆为同级目录：

```
RustroverProjects/
├─ SdvxOnEar/
└─ iidxOnKnitting/
```

随后 `cargo build -r` 即可（`FFMPEG_DIR` 已在 `.cargo/config.toml` 中配好，指向 `../iidxOnKnitting/vendor`；若放在别处需相应修改）。
首次 clean 构建依赖 LLVM + VS 2022 MSVC 工具链（这是 on knitting 静态链接裁剪版 FFmpeg 所需，`vendor/` 内已带预编译二进制，无需自行编译 FFmpeg），增量构建不需要。构建出的可执行文件已静态链接全部依赖，**运行时无需系统 FFmpeg 或任何外部库**。

### 运行

`SdvxOnEar -s <contents> [-d 输出] [-f] [-j N]`

| 参数 | 说明 |
|------|------|
| `-s, --src <路径>` | **必填**。SDVX 的 `contents` 文件夹；程序自动拼接 `data/music`、`data/others/music_db.xml`，并搜索 omnimix 补丁 |
| `-d, --dst <路径>` | 输出目录。省略时默认为当前工作目录 |
| `-f, --force` | 全量转换：对已存在于输出目录的歌曲也重新转换（默认只增量转换新增歌曲） |
| `-j, --jobs <N>` | 并发 worker 数量。省略时默认为逻辑 CPU 核心数 |

使用案例：

`SdvxOnEar -s C:\Game\SDVX\contents -d C:\Game\MUSIC -j 8`：读取电脑中的SDVX/contents文件夹中的游戏数据，输出到MUSIC文件夹中，开启8个并发进程。

## 已知问题

1. 由于BEMANI所使用的神奇SHIFT-JIS编码中使用了部分私有字形，导致EMOJI和带注音的拉丁字母在转换为UTF-8时会变成生僻汉字。这一点已经于`src/common.rs`进行了手动修改，新歌出现类似问题仍需手动补充`FIXUP_RULES`表。
2. 有多音源的歌曲（比如極圏），需要单独进行处理。新歌出现类似问题仍需手动补充`SPECIAL_TASKS`表。

---

# SDVX on Ear

## Overview

SDVX on Ear extracts the songs out of the SDVX game files into a neatly organized music library, attaching the matching metadata and cover art to every track.
It reads your `contents/` folder, converts the songs in the game files (including the omnimix folder) from their native format (`.s3v` WMA Pro, plus the early omni `.2dx`) to Opus (SDVX's native format has poor support for metadata tags), and sorts them into per-version folders.
After the game updates to a new version, just run the program again and it will incrementally convert the newly added songs.

The metadata attached to each song:

| Field | Vorbis tag | Source |
|-------|------------|--------|
| Title | `TITLE` | in-game song title |
| Artist | `ARTIST` | in-game artist name |
| Album | `ALBUM` | full name of the version it belongs to (e.g. `SOUND VOLTEX EXCEED GEAR`) |
| Album artist | `ALBUMARTIST` | fixed to `BEMANI`, so one version groups as a single album |
| Track number | `TRACKNUMBER` | the song's in-game id |
| Title sort | `TITLESORT` | title reading (half-width katakana) |
| Artist sort | `ARTISTSORT` | artist reading |
| Release date | `DATE` | e.g. `2020-07-02` |
| BPM | `BPM` | maximum BPM |
| Cover | embedded image | the highest difficulty's jacket (`jk_<id>_<difficulty>_b.png`) |

This program contains no information copyrighted © Konami Arcade Games.

## Usage

Downloading a release build is the recommended way to use this program.

### Build

This program embeds [IIDX on Knitting](https://github.com/Nyanm/iidxOnKnitting) as a path dependency. Clone the two repositories as sibling directories:

```
RustroverProjects/
├─ SdvxOnEar/
└─ iidxOnKnitting/
```

Then `cargo build -r` (`FFMPEG_DIR` is already set in `.cargo/config.toml` to point at `../iidxOnKnitting/vendor`; adjust it if you put the repo elsewhere). A first clean build requires the LLVM + VS 2022 MSVC toolchains (needed by on knitting's statically-linked, trimmed FFmpeg; the prebuilt binaries ship in `vendor/`, so there is no need to compile FFmpeg yourself); incremental builds do not. The resulting executable statically links every dependency, so **no system FFmpeg or any external library is needed at runtime**.

### Run

`SdvxOnEar -s <contents> [-d output] [-f] [-j N]`

| Argument | Description |
|----------|-------------|
| `-s, --src <path>` | **Required.** SDVX's `contents` folder; the program appends `data/music` and `data/others/music_db.xml`, and searches for an omnimix patch |
| `-d, --dst <path>` | Output directory. Defaults to the current working directory when omitted |
| `-f, --force` | Full conversion: re-convert songs even if they already exist in the output (by default only newly added songs are converted, incrementally) |
| `-j, --jobs <N>` | Number of concurrent workers. Defaults to the logical CPU core count when omitted |

Example:

`SdvxOnEar -s C:\Game\SDVX\contents -d C:\Game\MUSIC -j 8`: read the game data from the SDVX/contents folder on your computer, output to the MUSIC folder, and run with 8 concurrent workers.

## Known issues

1. Because the quirky SHIFT-JIS encoding BEMANI uses relies on some private-use glyphs, emoji and accented Latin letters turn into obscure kanji when decoded to UTF-8. This has been corrected by hand in `src/common.rs`; new songs hitting the same issue still need manual additions to the `FIXUP_RULES` table.
2. Songs with multiple audio sources (such as 極圏) need special handling. New songs hitting the same issue still need manual additions to the `SPECIAL_TASKS` table.