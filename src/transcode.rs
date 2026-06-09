//! FFI transcode: ASF/WMA-Pro → Ogg/Opus using the vendored static libav + libopus.
//!
//! Replaces the old `ffmpeg.exe` subprocess call with direct libav FFI via `ffmpeg-the-third`.
//! Pipeline: demux ASF → decode WMA Pro → resample → encode libopus → mux Ogg.
//!
//! This module is designed to be reused in iidxOnEar once .2dx outer-container unpacking is added;
//! the transcode loop itself is format-agnostic.

use std::path::Path;
use std::sync::Once;
use anyhow::{Context, Result};
use ffmpeg_the_third as ffmpeg;

// libopus target bitrate in bps; the source is ~384kbps lossy WMA Pro, so 192k Opus is effectively transparent.
const OPUS_BITRATE: u32 = 192_000;

// ffmpeg init is idempotent but not free — call it exactly once across all threads.
static INIT: Once = Once::new();

fn ensure_init() {
    INIT.call_once(|| {
        ffmpeg::init().expect("ffmpeg vendored init failed");
    });
}

/// Transcode an audio file to Ogg/Opus at the module-default bitrate.
pub fn transcode_to_opus(input: &Path, output: &Path) -> Result<()> {
    ensure_init();

    // ── ① open input ────────────────────────────────────────────────────────
    let mut ictx = ffmpeg::format::input(input)
        .with_context(|| format!("opening input {}", input.display()))?;

    // ── ② find the best audio stream ────────────────────────────────────────
    let input_stream = ictx
        .streams()
        .best(ffmpeg::media::Type::Audio)
        .ok_or_else(|| anyhow::anyhow!("no audio stream in {}", input.display()))?;
    let stream_index = input_stream.index();

    let context = ffmpeg::codec::context::Context::from_parameters(input_stream.parameters())
        .with_context(|| "creating decoder context from stream parameters")?;
    let mut decoder = context
        .decoder()
        .audio()
        .with_context(|| "opening audio decoder")?;

    let in_format = decoder.format();
    let in_layout = decoder.ch_layout();
    let in_rate = decoder.rate();

    // ── ③ configure libopus encoder ─────────────────────────────────────────
    let codec = ffmpeg::encoder::find_by_name("libopus")
        .ok_or_else(|| anyhow::anyhow!("libopus encoder not found in vendored build"))?;
    let ctx_enc = ffmpeg::codec::context::Context::new_with_codec(codec);
    let mut encoder = ctx_enc
        .encoder()
        .audio()
        .with_context(|| "creating libopus audio encoder")?;
    encoder.set_rate(48_000);
    encoder.set_ch_layout(ffmpeg::ChannelLayout::STEREO);
    encoder.set_format(ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed));
    encoder.set_bit_rate(OPUS_BITRATE as usize);
    encoder.set_time_base(ffmpeg::Rational(1, 48_000));
    let mut encoder = encoder
        .open()
        .with_context(|| "opening libopus encoder")?;

    // ── ④ set up resampler ──────────────────────────────────────────────────
    let mut resampler = ffmpeg::software::resampling::Context::get2(
        in_format,
        in_layout,
        in_rate,
        ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed),
        ffmpeg::ChannelLayout::STEREO,
        48_000,
    )
    .with_context(|| "creating resampler")?;

    // ── ⑤ set up output (Ogg/Opus by extension) ─────────────────────────────
    let mut octx = ffmpeg::format::output(output)
        .with_context(|| format!("creating output {}", output.display()))?;
    let mut out_stream = octx
        .add_stream(codec)
        .with_context(|| "adding output stream")?;
    out_stream.copy_parameters_from_context(encoder.as_ref());
    out_stream.set_time_base(ffmpeg::Rational(1, 48_000));
    octx
        .write_header()
        .with_context(|| format!("writing header for {}", output.display()))?;

    // ── ⑥ transcode loop ────────────────────────────────────────────────────
    const CHUNK_SIZE: usize = 960;
    const CHANNELS: usize = 2;
    let mut leftover: Vec<f32> = Vec::new();          // interleaved flt, not yet encoded
    let mut sample_pos: i64 = 0;                      // running PTS in output-sample units

    for result in ictx.packets() {
        let (stream, packet) = result.with_context(|| "reading packet")?;
        if stream.index() != stream_index {
            continue;
        }

        decoder
            .send_packet(&packet)
            .with_context(|| "sending packet to decoder")?;

        let mut decoded = unsafe { ffmpeg::Frame::empty() };
        while decoder.receive_frame(&mut decoded).is_ok() {
            let decoded_audio = ffmpeg::frame::Audio::from(decoded);
            let mut resampled = ffmpeg::frame::Audio::empty();
            resampler
                .run(&decoded_audio, &mut resampled)
                .with_context(|| "resampling frame")?;

            // Read stereo packed data correctly using the (f32, f32) plane type.
            leftover.extend_from_slice(read_stereo_packed(&resampled));

            sample_pos += drain_chunks(
                &mut leftover,
                CHUNK_SIZE,
                CHANNELS,
                &mut encoder,
                &mut octx,
                sample_pos,
            )
            .with_context(|| "encoding chunk")?;

            decoded = unsafe { ffmpeg::Frame::empty() };
        }
    }

    // ── ⑦ flush decoder ─────────────────────────────────────────────────────
    decoder
        .send_eof()
        .with_context(|| "flushing decoder")?;
    let mut decoded = unsafe { ffmpeg::Frame::empty() };
    while decoder.receive_frame(&mut decoded).is_ok() {
        let decoded_audio = ffmpeg::frame::Audio::from(decoded);
        let mut resampled = ffmpeg::frame::Audio::empty();
        resampler
            .run(&decoded_audio, &mut resampled)
            .with_context(|| "resampling flushed frame")?;
        leftover.extend_from_slice(read_stereo_packed(&resampled));
        sample_pos += drain_chunks(
            &mut leftover,
            CHUNK_SIZE,
            CHANNELS,
            &mut encoder,
            &mut octx,
            sample_pos,
        )
        .with_context(|| "encoding flushed chunk")?;
        decoded = unsafe { ffmpeg::Frame::empty() };
    }

    // Drain partial remainder (zero-padded to a full chunk).
    if !leftover.is_empty() {
        let pad = CHUNK_SIZE * CHANNELS - leftover.len();
        leftover.resize(leftover.len() + pad, 0.0_f32);
        drain_chunks(
            &mut leftover,
            CHUNK_SIZE,
            CHANNELS,
            &mut encoder,
            &mut octx,
            sample_pos,
        )
        .with_context(|| "encoding trailing chunk")?;
    }

    // ── ⑧ flush encoder ─────────────────────────────────────────────────────
    encoder
        .send_eof()
        .with_context(|| "flushing encoder")?;
    receive_and_mux(&mut encoder, &mut octx);

    // ── ⑨ finalize output ───────────────────────────────────────────────────
    octx
        .write_trailer()
        .with_context(|| "writing trailer")?;

    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Read packed-stereo frame data as a flat interleaved `&[f32]` slice.
///
/// For packed stereo, `plane::<(f32, f32)>(0)` returns `&[(f32, f32)]` of length
/// `nb_samples` — each tuple is one (L, R) pair.  The memory layout is identical
/// to a flat `&[f32]` of double length, so the transmute is sound.
fn read_stereo_packed(frame: &ffmpeg::frame::Audio) -> &[f32] {
    let stereo: &[(f32, f32)] = frame.plane::<(f32, f32)>(0);
    unsafe { std::slice::from_raw_parts(stereo.as_ptr() as *const f32, stereo.len() * 2) }
}

/// Drain as many complete `chunk_size`-sample frames from `buf`, set PTS on each,
/// send to the encoder, and mux.  Returns how many samples were consumed so the
/// caller can advance its running PTS counter.
fn drain_chunks(
    buf: &mut Vec<f32>,
    chunk_size: usize,
    channels: usize,
    encoder: &mut ffmpeg::encoder::audio::Encoder,
    octx: &mut ffmpeg::format::context::Output,
    mut pts: i64,
) -> Result<i64> {
    let elements_per_chunk = chunk_size * channels;
    let mut sent = 0i64;
    while buf.len() >= elements_per_chunk {
        let chunk_data: Vec<f32> = buf.drain(..elements_per_chunk).collect();
        let mut chunk_frame = make_opus_frame(&chunk_data, chunk_size)?;
        chunk_frame.set_pts(Some(pts));
        encoder.send_frame(&chunk_frame)?;
        receive_and_mux(encoder, octx);
        pts += chunk_size as i64;
        sent += chunk_size as i64;
    }
    Ok(sent)
}

/// Allocate a packed-stereo-flt AVFrame with `samples` samples and copy the
/// interleaved data in.
fn make_opus_frame(data: &[f32], samples: usize) -> Result<ffmpeg::frame::Audio, anyhow::Error> {
    assert_eq!(data.len(), samples * 2);
    let mut audio = ffmpeg::frame::Audio::empty();
    audio.set_format(ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed));
    audio.set_rate(48_000);
    audio.set_samples(samples);
    audio.set_ch_layout(ffmpeg::ChannelLayout::STEREO);
    unsafe {
        av_frame_get_buffer(audio.as_mut_ptr(), 0);
    }
    let dst: &mut [(f32, f32)] = audio.plane_mut::<(f32, f32)>(0);
    assert_eq!(dst.len(), samples);
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr(), dst.as_mut_ptr() as *mut f32, data.len());
    }
    Ok(audio)
}

/// Drain encoded packets from the encoder and mux them into the output.
fn receive_and_mux(
    encoder: &mut ffmpeg::encoder::audio::Encoder,
    octx: &mut ffmpeg::format::context::Output,
) {
    let mut encoded = ffmpeg::Packet::empty();
    while encoder.receive_packet(&mut encoded).is_ok() {
        encoded.set_stream(0);
        if let Err(e) = encoded.write_interleaved(octx) {
            eprintln!("transcode warning: writing interleaved packet failed: {e}");
        }
        encoded = ffmpeg::Packet::empty();
    }
}

// `av_frame_get_buffer` is not re-exported through the safe wrapper.
unsafe extern "C" {
    fn av_frame_get_buffer(
        frame: *mut ffmpeg_the_third::ffi::AVFrame,
        align: std::ffi::c_int,
    ) -> std::ffi::c_int;
}
