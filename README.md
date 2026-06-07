# SDVX on Ear

**！！！请确保程序运行目录或PATH下已有FFmpeg！！！**

## 简介

SDVX on Ear是一款用于将SDVX游戏文件中的歌曲提取为组织好的音乐文件仓库的程序，同时会为每一首歌加入对应的元信息与封面。
程序会尝试读取你的`contents/`文件夹，将游戏文件中的歌曲（包括omnimix文件夹）从WMA转换为Opus格式（SDVX原生的WMA格式对于元信息标签的支持很差），再依据版本放入对应的文件夹中。
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

可以直接下载release版本，或者使用`cargo build -r`进行编译。程序依赖FFmpeg（需要带有libopus，但一般发行版本都有）进行乐曲文件的转换，因此，在程序运行之前，请务必确保当前工作目录或者PATH中有FFmpeg程序。

程序的参数如下：

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

1. 一些早期的omni曲目的音频格式依然是`.2dx`，这些歌曲的音频转换还未实现，现在会跳过。
2. 由于BEMANI所使用的神奇SHIFT-JIS编码中使用了部分私有字形，导致EMOJI和带注音的拉丁字母在转换为UTF-8时会变成生僻汉字。这一点已经于`src/common.rs`进行了手动修改，新歌出现类似问题仍需手动补充`FIXUP_RULES`表。
3. 有多音源的歌曲（比如極圏），需要单独进行处理。新歌出现类似问题仍需手动补充`SPECIAL_TASKS`表。

---

# SDVX on Ear

**!!! Make sure FFmpeg is available in the working directory or on PATH before running !!!**

## Overview

SDVX on Ear extracts the songs out of the SDVX game files into a neatly organized music library, attaching the matching metadata and cover art to every track.
It reads your `contents/` folder, converts the songs in the game files (including the omnimix folder) from WMA to Opus (SDVX's native WMA has poor support for metadata tags), and sorts them into per-version folders.
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

Download a release build, or compile it yourself with `cargo build -r`. The program relies on FFmpeg (built with libopus, which most distributions include) to convert the audio, so before running, make sure FFmpeg is present in the current working directory or on PATH.

The program's arguments:

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

1. Some early omni tracks still use the `.2dx` audio format; converting these is not yet implemented, so they are skipped for now.
2. Because the quirky SHIFT-JIS encoding BEMANI uses relies on some private-use glyphs, emoji and accented Latin letters turn into obscure kanji when decoded to UTF-8. This has been corrected by hand in `src/common.rs`; new songs hitting the same issue still need manual additions to the `FIXUP_RULES` table.
3. Songs with multiple audio sources (such as 極圏) need special handling. New songs hitting the same issue still need manual additions to the `SPECIAL_TASKS` table.