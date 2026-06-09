# 自包含静态 libav + libopus 构建与 FFI 链接 Runbook

> 目的：把 SDVX/IIDX 这类工具做成**单 exe 自包含**——不再依赖系统/同目录的 `ffmpeg.exe`，而是把裁剪过的 **静态 libav（avcodec/avformat/avutil/swresample）+ libopus** 编进 `vendor/`，用 `ffmpeg-the-third` 做 FFI 链接。
>
> 本文档基于 SdvxOnEar 的实战记录（2026-06），所有命令都是验证过能跑通的。**迁移到 iidxOnEar 时整套流程可直接复用**（裁剪范围里已经预留了 `.2dx` 用的 MS-ADPCM / WAV 解码器，见 §3）。
>
> 环境：Windows 10/11 + MSVC（x64）+ Rust MSVC 工具链。

---

## 目录

1. [总体思路与裁剪范围](#1-总体思路与裁剪范围)
2. [前置工具清单（含安装命令）](#2-前置工具清单)
3. [关键裁剪决策](#3-关键裁剪决策)
4. [构建环境的"咒语"（最容易踩坑的部分）](#4-构建环境的咒语)
5. [编译 libopus（静态）](#5-编译-libopus静态)
6. [编译 ffmpeg（裁剪静态 + libopus）](#6-编译-ffmpeg裁剪静态--libopus)
7. [装配 vendor/](#7-装配-vendor)
8. [Rust 侧 FFI 接线](#8-rust-侧-ffi-接线)
9. [crate ↔ ffmpeg 版本必须匹配（核心坑）](#9-crate--ffmpeg-版本必须匹配核心坑)
10. [验证（链接冒烟测试）](#10-验证链接冒烟测试)
11. [迁移到 iidxOnEar 的注意事项](#11-迁移到-iidxonear-的注意事项)
12. [常见坑速查表](#12-常见坑速查表)

---

## 1. 总体思路与裁剪范围

`s3v`（SDVX）实际是 **ASF 容器 / WMA Pro 编码**的有损音频。目标管线：

```
输入文件(ASF/RIFF) → 解复用 → 解码成 PCM → 重采样 44.1k→48k → libopus 编码 → Ogg-Opus 封装 → .opus
```

- 解码、解复用、重采样、封装：全部由 **ffmpeg 的 libav** 完成。
- Opus 编码这一步：ffmpeg 自带的原生 opus 编码器质量差，所以**外挂官方 libopus**（`--enable-libopus`）。这就是为什么要**先单独编 libopus，再编 ffmpeg**（详见姊妹文档 / 记忆：libopus 与 ffmpeg 是两个仓库，ffmpeg 当"包工头"，把 PCM→Opus 这一格分包给 libopus）。
- 标签/封面：仍用 Rust 的 `lofty`，不经过 ffmpeg。

最终产物：`vendor/`（含 5 个 `.lib` + 头文件），提交进 git；Rust 用 `ffmpeg-the-third` 指向它静态链接。

---

## 2. 前置工具清单

| 工具 | 用途 | 安装方式 | 落点 |
|---|---|---|---|
| **Visual Studio 2022**（含 C++ 工作负载） | MSVC `cl`/`link`/`lib` | VS Installer | `C:\Program Files\Microsoft Visual Studio\2022\Community` |
| **MSYS2** | 提供 `bash`/`make`/`pkgconf` 跑 ffmpeg 的 configure | msys2.org | `C:\msys64` |
| └ pacman 包 | `make diffutils pkgconf` | `pacman -S make diffutils pkgconf` | `C:\msys64\usr\bin` |
| **原生 Windows CMake** | 编 libopus（带 "Visual Studio 17 2022" 生成器） | cmake.org / `winget install Kitware.CMake` | `C:\Program Files\CMake\bin` |
| **原生 LLVM**（libclang） | `ffmpeg-sys-the-third` 的 bindgen 需要 | `winget install LLVM.LLVM` | `C:\Program Files\LLVM\bin\libclang.dll` |
| **ffmpeg 源码（正式发布 tarball）** | 见 §9，**不要用 git master** | ffmpeg.org/releases/`ffmpeg-8.0.tar.xz` | 解压到 `.ffmpeg/` |
| **libopus 源码** | 建议 checkout 一个 release tag | `git clone https://github.com/xiph/opus.git`（切 `v1.5.2`） | 解压到 `.opus/` |

> ⚠️ **不需要 nasm**。我们用 `--disable-asm` 跳过汇编优化（体积/速度对本场景无所谓，省掉 nasm 依赖）。
>
> `.ffmpeg/` 和 `.opus/` 都加进 `.gitignore`（只提交 `vendor/`）。

---

## 3. 关键裁剪决策

ffmpeg `configure` 用 `--disable-everything` 全关，再按需 `--enable-` 打开。本项目开启的组件：

| 组件 | flag | 用途 |
|---|---|---|
| WMA Pro 解码器 | `--enable-decoder=wmapro` | **SDVX s3v** 解码 |
| ASF 解复用器 | `--enable-demuxer=asf` | s3v 容器 |
| MS-ADPCM 解码器 | `--enable-decoder=adpcm_ms` | **IIDX `.2dx`**（RIFF/WAVE MS-ADPCM）预留 |
| PCM s16le 解码器 | `--enable-decoder=pcm_s16le` | WAV 裸 PCM |
| WAV 解复用器 | `--enable-demuxer=wav` | `.2dx` 解包后的 RIFF/WAVE |
| libopus 编码器 | `--enable-libopus --enable-encoder=libopus` | Opus 编码（外挂 libopus） |
| Opus 封装器 | `--enable-muxer=opus` | `.opus`（Ogg 封装） |
| 文件协议 | `--enable-protocol=file` | 读写本地文件 |
| swresample | （默认开，不要关） | 44.1k/fltp → 48k 重采样 |

关掉的大件：`--disable-programs --disable-doc --disable-network --disable-autodetect --disable-avdevice --disable-avfilter --disable-swscale`。

> **IIDX 复用**：adpcm_ms / wav / pcm_s16le 已经编进去了。迁移时这套裁剪**基本不用改**——除非 IIDX 的某些曲目用了别的编码（届时加对应 `--enable-decoder=`）。

---

## 4. 构建环境的"咒语"

> 这一节是整个流程**最坑**的部分，全是血泪。ffmpeg 的 `configure` 必须在 bash（MSYS2）里跑，但编译又要用 MSVC，两者的环境拼接非常容易出问题。

### 4.1 用 `vcvars64.bat`，别用 `Enter-VsDevShell`

PowerShell 的 `Enter-VsDevShell` 依赖 `vswhere.exe`，本机缺它就会**静默地不把 `cl`/`lib` 加进 PATH**。改用经典的 `vcvars64.bat`（自带兜底，能正确设置 PATH/INCLUDE/LIB）。

### 4.2 ⚠️ 最大的坑：`bash.exe -c "bash <脚本>"` 会跑进 WSL，不是 MSYS2！

内层那个**裸 `bash`** 经 PATH 解析会命中 `C:\Windows\System32\bash.exe`（WSL 启动器），而不是 MSYS2。判断自己掉进 WSL 的特征：

- `pwd` 显示 `/mnt/c/...`（MSYS2 是 `/c/...`）
- `uname` 显示 `...-Microsoft ... GNU/Linux`；`mount` 出现 `wslfs`
- 找不到 `cl`、`INCLUDE`/`LIB` 为空、`link`/`make` 是 Linux 版

**修复**：用 MSYS2 bash 的**全路径**直接执行脚本，**绝不嵌套裸 `bash`**：
```
C:\msys64\usr\bin\bash.exe "脚本.sh"
```

### 4.3 包装批处理 `build_env.bat`（放在 `.ffmpeg/` 里，**CRLF 行尾**）

```bat
@echo off
cd /d "%~dp0"
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
set "PATH=%PATH%;C:\msys64\usr\bin"
set MSYS2_PATH_TYPE=inherit
C:\msys64\usr\bin\bash.exe "%~1"
```

从 PowerShell 调用：
```powershell
cmd /c "C:\...\.ffmpeg\build_env.bat build_ff.sh"
```

要点：
- `set "PATH=%PATH%;C:\msys64\usr\bin"` —— msys 追加在 **VS 之后**，这样 MSVC 的 `link.exe`/`lib.exe` 压过 msys 的 `/usr/bin/link`（MSYS2 把 `/usr/bin` 放在转换后 PATH 的**末尾**，所以 MSVC 工具优先）。
- `MSYS2_PATH_TYPE=inherit` —— 让 bash 继承 Windows 的 PATH/INCLUDE/LIB（vcvars 设的那些）。
- `cd /d "%~dp0"` —— 切到批处理自身所在目录，消除 cwd 依赖。
- 出现 `'vswhere.exe' is not recognized` 一行是**无害噪音**，只要后面有 `Environment initialized for: 'x64'` 就 OK。

### 4.4 脚本行尾约定

- **`.bat` 用 CRLF**：写完 `sed -i 's/$/\r/' build_env.bat`。
- **`.sh` 用 LF**：写完 `sed -i 's/\r$//' *.sh`（CRLF 会让 `cd /c/.../dir\r` 路径里多个回车符 → "No such file or directory"）。

### 4.5 捕获输出

PowerShell 捕获 bash 的多行 stdout 经常**截断**。可靠做法：脚本把输出重定向到**相对路径**的 log 文件，再用编辑器/Read 读 log。

---

## 5. 编译 libopus（静态）

libopus 用**原生 Windows CMake + "Visual Studio 17 2022" 生成器**（它自己管理 MSVC 环境，**完全不用 bash**，直接在 PowerShell 跑）。MSYS2 自带的 cmake 不行（没有 VS 生成器，Ninja+cl 探测也过不去）。

```powershell
$cm  = 'C:\Program Files\CMake\bin\cmake.exe'
$src = 'C:\Users\<you>\...\.opus'      # opus 源码目录
& $cm -S $src -B "$src\build" -G "Visual Studio 17 2022" -A x64 `
  -DOPUS_BUILD_SHARED_LIBRARY=OFF -DOPUS_BUILD_TESTING=OFF -DOPUS_BUILD_PROGRAMS=OFF `
  -DCMAKE_INSTALL_PREFIX="$src\install"
& $cm --build   "$src\build" --config Release --parallel
& $cm --install "$src\build" --config Release
```

产物：`.opus/install/lib/opus.lib`（约 1.2MB）+ `.opus/install/include/opus/*.h`。

要点：
- **CRT 用默认的 `/MD`（动态运行库）**——和 ffmpeg msvc 工具链、Rust MSVC 默认一致。**不要**开 `OPUS_STATIC_RUNTIME`，否则 CRT 不匹配链接报错。
- opus 的 `cmake_minimum_required` 是 3.16，cmake 4.x 直接兼容，**不要**传 `CMAKE_POLICY_VERSION_MINIMUM`（而且 PowerShell 5.1 会把 `=3.5` 拆成 `3` 和 `.5` 反而报错）。
- 如果是浅克隆没 tag，opus 版本探测会失败（`Version: 0`），无害；建议 `git checkout v1.5.2` 拿到正常版本号。

---

## 6. 编译 ffmpeg（裁剪静态 + libopus）

### 6.1 先写一份 `vendor/lib/pkgconfig/opus.pc`（给 ffmpeg 的 `--enable-libopus` 用）

`--enable-libopus` 走 pkg-config 找 libopus。手写一份（内部路径用 `C:/` 正斜杠，`cl`/`link` 认）：

```ini
prefix=C:/Users/<you>/.../vendor
exec_prefix=${prefix}
libdir=${prefix}/lib
includedir=${prefix}/include

Name: Opus
Description: Opus IETF audio codec
URL: https://opus-codec.org/
Version: 1.5.2
Libs: -L${libdir} -lopus
Cflags: -I${includedir}/opus
```

> `PKG_CONFIG_PATH` 环境变量要用 **`/c/...` 的 msys 形式**（因为 `:` 是路径分隔符，`C:` 里的冒号会被误拆）。但 `.pc` **内部**的 `-I`/`-L` 用 `C:/...`（直接喂给 cl/link）。

### 6.2 构建脚本 `build_ff.sh`（LF 行尾，放 `.ffmpeg/`）

```bash
set -e
cd /c/Users/<you>/.../.ffmpeg
export PKG_CONFIG_PATH=/c/Users/<you>/.../vendor/lib/pkgconfig

make distclean 2>/dev/null || true
./configure \
  --toolchain=msvc --enable-static --disable-shared \
  --disable-programs --disable-doc --disable-network --disable-autodetect \
  --disable-avdevice --disable-avfilter --disable-swscale --disable-everything \
  --enable-decoder=wmapro --enable-demuxer=asf \
  --enable-decoder=adpcm_ms --enable-decoder=pcm_s16le --enable-demuxer=wav \
  --enable-protocol=file --disable-asm \
  --enable-libopus --enable-encoder=libopus --enable-muxer=opus \
  --pkg-config=pkgconf --pkg-config-flags=--static
make -j8
make install prefix=/c/Users/<you>/.../.ffmpeg/_install
```

跑：`cmd /c "C:\...\.ffmpeg\build_env.bat build_ff.sh"`。

要点：
- `--toolchain=msvc` 让 ffmpeg 用 `cl`/`link`/`lib`。
- `--disable-postproc` 在 ffmpeg 8.0 会**被拒绝**（postproc 默认就是关的），**别加**。
- `swresample` 默认开，**别关**（44.1k/fltp → 48k 必须）。
- `--pkg-config=pkgconf`：直接用 `pkgconf`（`pkg-config` 在 msys 里是软链接，Windows 式 PATH 查找可能认不出）。
- 验证组件是否真启用：ffmpeg 8.0 把组件级宏放在 **`config_components.h`**（不是 `config.h`！`config.h` 只有库级的 `CONFIG_SWRESAMPLE` 等）。`grep CONFIG_LIBOPUS_ENCODER config_components.h` 应为 `1`。

---

## 7. 装配 vendor/

`make install` 把库装到 `_install/lib/`。**注意 ffmpeg 8.0.x 的 msvc 工具链把 `LIBSUF` 设成了 `.a`**（`config.mak` 里 `AR_CMD=lib.exe`）——也就是说这些 `lib<name>.a` **其实是 lib.exe 造的 MSVC COFF 库**，只是名字是 `.a`。

而 MSVC 的 rustc 解析 `cargo:rustc-link-lib=static=avcodec` 时找的是 **`avcodec.lib`**。所以汇入 vendor 时要**改名**（去 `lib` 前缀、换 `.lib` 后缀）：

```bash
SRC=.ffmpeg/_install
cp "$SRC/lib/libavcodec.a"    vendor/lib/avcodec.lib
cp "$SRC/lib/libavformat.a"   vendor/lib/avformat.lib
cp "$SRC/lib/libavutil.a"     vendor/lib/avutil.lib
cp "$SRC/lib/libswresample.a" vendor/lib/swresample.lib
# opus.lib 已在 vendor/lib/（来自 §5）
rm -rf vendor/include/libav*    # 清旧头
cp -r "$SRC"/include/libav* vendor/include/
```

> 不同 ffmpeg 版本的 `LIBSUF` 可能是 `.a` 也可能直接是 `.lib`（无 `lib` 前缀），**无论哪种都改成 `<name>.lib` 最保险**。

最终布局（**这个目录提交进 git**，约 17MB）：

```
vendor/
├─ lib/
│   avcodec.lib  avformat.lib  avutil.lib  swresample.lib  opus.lib
│   pkgconfig/opus.pc          # 仅构建期用；Rust 的 FFMPEG_DIR 模式不读它
└─ include/
    libavcodec/  libavformat/  libavutil/  libswresample/  opus/
```

---

## 8. Rust 侧 FFI 接线

### 8.1 `Cargo.toml`

```toml
[dependencies]
ffmpeg-the-third = { version = "4.1.0", default-features = false, features = ["codec", "format", "software-resampling", "static"] }
```

- **`default-features = false` 至关重要**：默认 feature 会拉 avdevice/avfilter/swscale/postproc，而我们没编这些 → 链接失败。
- 只开 `codec`(avcodec) / `format`(avformat) / `software-resampling`(swresample)；avutil 是隐式基座。
- `static` 强制静态链接。
- 版本号见 §9。

### 8.2 `.cargo/config.toml`

```toml
[env]
FFMPEG_DIR = { value = "vendor", relative = true }
LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
```

- `FFMPEG_DIR` 用 `relative = true`，解析为 `<项目根>/vendor`——**可移植**，不写死机器路径。`ffmpeg-sys-the-third` 读它：用 `$DIR/include` 跑 bindgen、`$DIR/lib` 做链接，并**跳过 pkg-config**。
- `LIBCLANG_PATH` 给 bindgen 找 `libclang.dll`。没加 `force = true`，所以用户自己设的环境变量会优先。

### 8.3 `build.rs`（项目根）

`ffmpeg-sys-the-third` 在 FFMPEG_DIR 模式下**不会**链 libopus，也**不会**链 Windows 系统库，得自己补：

```rust
use std::env;

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");

    // libopus：avcodec.lib 引用了 opus_* 符号，最终链接必须带上 opus.lib
    println!("cargo:rustc-link-search=native={manifest}/vendor/lib");
    println!("cargo:rustc-link-lib=static=opus");

    // 静态 libav 在 Windows 上依赖的系统库（如 avutil 的 BCryptGenRandom）
    // 多列几个无害——没被引用的库 MSVC 链接器会忽略
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        for lib in ["bcrypt", "user32", "ole32", "ws2_32", "secur32", "advapi32", "shell32"] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    }
}
```

---

## 9. crate ↔ ffmpeg 版本必须匹配（核心坑）

**`ffmpeg-the-third` 的安全封装层对 libav 的枚举（`AVCodecID`、`AVColorPrimaries`、`AVPacketSideDataType`…）写了一堆穷尽 `match`，是按 ffmpeg 的某个 _正式发布_ 写死的，不是 git master。**

如果你用 ffmpeg **git master 快照**（`RELEASE` 显示 `X.Y.git`），它会带很多比正式版新的枚举变体（如 `AV_CODEC_ID_JPEGXS`、`IAMF_*`、`AVCOL_PRI_EXT_BASE`…），crate 的 `match` 覆盖不到 → **crate 自身编译报 `E0004 non-exhaustive patterns`**（5 处左右），根本到不了链接。

**修复：ffmpeg 源码用和 crate 对应的正式发布 tarball。** 对应关系（cargo 的 `version` 会忽略 `+` 后的构建元数据，只按主版本匹配）：

| crate 版本 | 对应 ffmpeg | libavcodec |
|---|---|---|
| `ffmpeg-the-third = "4.1.0"` | **8.0**（用 `ffmpeg-8.0.2.tar.xz` 等 8.0.x） | 62.11.x |
| `ffmpeg-the-third = "5.0.0"` | 8.1 | — |

本项目用的是 **ffmpeg 8.0.2 + crate 4.1.0**，干净通过。

> **改了 `vendor/include` 头文件后**，必须 `cargo clean -p ffmpeg-sys-the-third -p ffmpeg-the-third` 再 build——bindgen 会缓存 `bindings.rs`，**不会自动感知头文件内容变化**，否则会拿旧绑定继续报同样的错。

---

## 10. 验证（链接冒烟测试）

写个临时 `src/bin/ffsmoke.rs`：

```rust
use ffmpeg_the_third as ffmpeg;

fn main() {
    ffmpeg::init().expect("ffmpeg init failed");
    println!("wmapro decoder : {:?}", ffmpeg::decoder::find(ffmpeg::codec::Id::WMAPRO).map(|c| c.name().to_string()));
    println!("libopus encoder: {:?}", ffmpeg::encoder::find_by_name("libopus").map(|c| c.name().to_string()));
    println!("opus muxer     : {}", if ffmpeg::format::output(&std::path::Path::new("probe.opus")).is_ok() { "OK" } else { "MISSING" });
    let _ = std::fs::remove_file("probe.opus");
}
```

构建+运行（**首次构建从 VS 开发环境跑**，让 clang 能通过 `INCLUDE` 找到 MSVC 系统头文件）：

```powershell
cmd /c '"C:\...\vcvars64.bat" && cargo build --bin ffsmoke'
.\target\debug\ffsmoke.exe
```

期望输出：
```
wmapro decoder : Some("wmapro")
libopus encoder: Some("libopus")
opus muxer     : OK
```

跑出来 = 整条 vendored 静态 FFI 链路（编译 + bindgen + 链接 + 运行）全部成立，且 exe **不依赖任何外部 ffmpeg**。验证完删掉 `ffsmoke.rs`。

> **构建环境待确认**：不开 VS 开发命令行、直接 `cargo build` 能否成功取决于 clang 能否自动探测到 VS 的系统头文件。`bindings.rs` 一旦生成，后续增量构建就不再需要 clang。发布前应确认 clean `cargo build -r` 是否需要 VS 环境；若需要，在 README 注明"从 VS 开发命令行构建"。

---

## 11. 迁移到 iidxOnEar 的注意事项

整套构建流程（§4–§8）**原样复用**。差异点：

1. **音频格式**：IIDX 早期是 `.2dx`（2DX9 归档，内含 RIFF/WAVE 的 MS-ADPCM 或裸 PCM）。
   - libav 侧：`adpcm_ms` / `pcm_s16le` / `wav` 解码解复用器**已经编进去了**（§3），大概率不用改 ffmpeg 裁剪。
   - 但 `.2dx` 的**外层归档（2DX9）需要自己在 Rust 里解包**，取出里面的 RIFF/WAVE 再喂给 libav。这部分是 IIDX 独有逻辑，与本文档无关。
   - 若 IIDX 新曲用了别的编码，加对应 `--enable-decoder=` 重编即可。
2. **vendor/ 可以共享**：如果 iidxOnEar 和 SdvxOnEar 的裁剪范围一致，**直接把本项目的 `vendor/` 拷过去**，连重编都省了。只有当需要新增编解码器时才重走 §5–§7。
3. **Rust 接线（§8）完全一样**：`Cargo.toml` / `.cargo/config.toml` / `build.rs` 照抄。
4. **版本匹配（§9）照旧**：ffmpeg 正式发布 + 对应 crate 版本。

---

## 12. 常见坑速查表

| 症状 | 原因 | 解法 |
|---|---|---|
| crate 自身报 `E0004 non-exhaustive patterns` | ffmpeg 用了 git master，枚举比 crate 新 | 换正式发布 tarball（§9） |
| `cl: command not found`（bash 里） | 掉进 WSL 了 / vswhere 缺失没设好 VS 环境 | 单层 msys2 bash 全路径（§4.2）+ vcvars64.bat（§4.1） |
| `pwd` 是 `/mnt/c/...`、`mount` 有 `wslfs` | 跑的是 WSL 不是 MSYS2 | 同上，别嵌套裸 `bash` |
| `cd /c/...: No such file or directory` | `.sh` 是 CRLF，路径尾多了 `\r` | `sed -i 's/\r$//' *.sh` |
| `--disable-postproc` 配置报错 | ffmpeg 8.0 拒绝该 flag | 删掉它（postproc 默认就关） |
| rustc 找不到 `avcodec.lib` | vendor 里是 `libavcodec.a` | 改名 `<name>.lib`（§7） |
| 链接报 CRT 不匹配 | libopus 用了 `/MT` 静态 CRT | opus 别开 `OPUS_STATIC_RUNTIME`，用默认 `/MD`（§5） |
| 链接缺 `opus_*` 符号 | 没链 libopus | build.rs 加 `rustc-link-lib=static=opus`（§8.3） |
| 链接缺 `BCryptGenRandom` 等 Win32 符号 | 没链系统库 | build.rs 补 bcrypt/user32 等（§8.3） |
| 链接拉 avfilter/avdevice/swscale 失败 | crate 默认 feature 开了它们 | `default-features = false`（§8.1） |
| `bindgen` 报找不到 `libclang` | 没装 LLVM | `winget install LLVM.LLVM` + 设 `LIBCLANG_PATH`（§2/§8.2） |
| 改了头文件但还报旧枚举错 | bindgen 缓存了旧 `bindings.rs` | `cargo clean -p ffmpeg-sys-the-third -p ffmpeg-the-third`（§9） |
| `pkg-config`/`find` 行为诡异 | bash 命中了 Windows 同名命令（System32 在 PATH 靠前） | 用 `pkgconf`；列文件别用裸 `find`（用 GNU find 全路径或在 cwd 用相对） |

---

*最后更新：2026-06-09 — 基于 SdvxOnEar 实战（ffmpeg 8.0.2 + libopus 1.5.x + ffmpeg-the-third 4.1.0 + MSVC x64）。*
