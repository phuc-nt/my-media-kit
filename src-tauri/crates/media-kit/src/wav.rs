//! Minimal WAV parser for the specific output ffmpeg produces when we ask
//! for `pcm_f32le` + `wav` container. We only need to find the `data` chunk
//! and interpret its bytes as `Vec<f32>`; we do not support every WAV
//! variant in the wild.
//!
//! This lives in media-kit rather than transcription-kit so both the
//! transcription pipeline and silence detection can share it (they both
//! need raw f32 samples).

use crate::error::MediaError;
use crate::{TARGET_CHANNELS, TARGET_SAMPLE_RATE};

/// Parse a WAV byte buffer that must be:
///   - RIFF header
///   - `fmt ` subchunk describing format 3 (IEEE float), `TARGET_CHANNELS`
///     channels at `TARGET_SAMPLE_RATE` Hz, 32 bit
///   - `data` subchunk with little-endian f32 samples
///
/// Returns the flat sample buffer.
pub fn parse_wav_f32_mono(buf: &[u8]) -> Result<Vec<f32>, MediaError> {
    if buf.len() < 44 || &buf[0..4] != b"RIFF" || &buf[8..12] != b"WAVE" {
        return Err(MediaError::PcmRead("missing RIFF/WAVE header".into()));
    }

    let mut offset = 12;
    let mut fmt_ok = false;

    while offset + 8 <= buf.len() {
        let chunk_id = &buf[offset..offset + 4];
        let chunk_size = u32::from_le_bytes(
            buf[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| MediaError::PcmRead("chunk size parse failed".into()))?,
        ) as usize;
        offset += 8;

        match chunk_id {
            b"fmt " => {
                if chunk_size < 16 || offset + chunk_size > buf.len() {
                    return Err(MediaError::PcmRead("short fmt chunk".into()));
                }
                let format_tag = u16::from_le_bytes([buf[offset], buf[offset + 1]]);
                let channels = u16::from_le_bytes([buf[offset + 2], buf[offset + 3]]);
                let sample_rate = u32::from_le_bytes(
                    buf[offset + 4..offset + 8]
                        .try_into()
                        .map_err(|_| MediaError::PcmRead("sample rate parse".into()))?,
                );
                let bits_per_sample = u16::from_le_bytes([buf[offset + 14], buf[offset + 15]]);

                // format_tag 0x0003 = IEEE float, 0xFFFE = extensible (we
                // accept if the other fields look right — ffmpeg sometimes
                // emits extensible with float sub-format).
                if format_tag != 0x0003 && format_tag != 0xFFFE {
                    return Err(MediaError::PcmRead(format!(
                        "unexpected format tag: {format_tag:#06x}"
                    )));
                }
                if channels != TARGET_CHANNELS {
                    return Err(MediaError::PcmRead(format!(
                        "expected {} channels, got {channels}",
                        TARGET_CHANNELS
                    )));
                }
                if sample_rate != TARGET_SAMPLE_RATE {
                    return Err(MediaError::PcmRead(format!(
                        "expected {} Hz, got {sample_rate}",
                        TARGET_SAMPLE_RATE
                    )));
                }
                if bits_per_sample != 32 {
                    return Err(MediaError::PcmRead(format!(
                        "expected 32-bit samples, got {bits_per_sample}"
                    )));
                }
                fmt_ok = true;
                offset += chunk_size;
                // WAV chunks are word-aligned.
                if chunk_size & 1 == 1 && offset < buf.len() {
                    offset += 1;
                }
            }
            b"data" => {
                if !fmt_ok {
                    return Err(MediaError::PcmRead("data chunk before fmt chunk".into()));
                }
                // ffmpeg streaming mode writes sentinel sizes (0 / 0xFFFFFFFF)
                // when the total length isn't known before encode. Fall back
                // to "read everything left in the buffer" in that case.
                let remaining = buf.len() - offset;
                let payload_len = if chunk_size == 0
                    || chunk_size == u32::MAX as usize
                    || offset + chunk_size > buf.len()
                {
                    remaining
                } else {
                    chunk_size
                };
                let end = offset + payload_len;
                let payload = &buf[offset..end];
                if payload.len() % 4 != 0 {
                    return Err(MediaError::PcmRead(
                        "data chunk not a multiple of 4 bytes".into(),
                    ));
                }
                let mut samples = Vec::with_capacity(payload.len() / 4);
                for s in payload.chunks_exact(4) {
                    samples.push(f32::from_le_bytes([s[0], s[1], s[2], s[3]]));
                }
                return Ok(samples);
            }
            _ => {
                // Skip unknown chunk (LIST, bext, etc.)
                offset = offset.saturating_add(chunk_size);
                if chunk_size & 1 == 1 && offset < buf.len() {
                    offset += 1;
                }
            }
        }
    }

    Err(MediaError::PcmRead("no data chunk found".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_fake_wav(samples: &[f32]) -> Vec<u8> {
        let data_bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let data_len = data_bytes.len() as u32;
        let riff_size = 36 + data_len;

        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&riff_size.to_le_bytes());
        out.extend_from_slice(b"WAVE");

        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes()); // chunk size
        out.extend_from_slice(&3u16.to_le_bytes()); // format tag (IEEE float)
        out.extend_from_slice(&1u16.to_le_bytes()); // channels
        out.extend_from_slice(&16_000u32.to_le_bytes()); // sample rate
        out.extend_from_slice(&(16_000u32 * 4).to_le_bytes()); // byte rate
        out.extend_from_slice(&4u16.to_le_bytes()); // block align
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per sample

        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        out.extend_from_slice(&data_bytes);
        out
    }

    #[test]
    fn parses_simple_wav() {
        let input = [0.0_f32, 0.5, -0.5, 1.0, -1.0];
        let wav = build_fake_wav(&input);
        let parsed = parse_wav_f32_mono(&wav).unwrap();
        assert_eq!(parsed, input);
    }

    #[test]
    fn rejects_bad_header() {
        let err = parse_wav_f32_mono(&[0u8; 12]).unwrap_err();
        assert!(matches!(err, MediaError::PcmRead(_)));
    }

    #[test]
    fn rejects_wrong_sample_rate() {
        let mut wav = build_fake_wav(&[0.0, 0.0]);
        // Patch sample rate field (offset 24 inside fmt chunk payload).
        let rate_offset = 12 + 8 + 4;
        wav[rate_offset..rate_offset + 4].copy_from_slice(&48_000u32.to_le_bytes());
        assert!(parse_wav_f32_mono(&wav).is_err());
    }
}
