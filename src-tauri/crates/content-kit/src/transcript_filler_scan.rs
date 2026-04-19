//! Rule-based filler detection using Whisper word-level timestamps.
//!
//! No AI/LM server required. Scans `WordTimestamp` entries in each
//! `TranscriptionSegment` against the known EN/VI filler lists and returns
//! `FillerDetection` objects with precise, word-level cut ranges.
//!
//! Adjacent filler words within `merge_gap_ms` are merged into one detection
//! so that "ừm … à" with a short gap becomes a single cut rather than two.

use creator_core::{FillerDetection, TranscriptionSegment};

const EN_FILLERS: &[&str] = &[
    "um", "uh", "er", "ah", "hmm", "like", "you know", "i mean", "basically",
    "actually", "literally", "right", "so", "well", "kind of", "sort of",
    "anyway", "obviously",
];

const VI_FILLERS: &[&str] = &[
    "ờ", "à", "ừm", "ừ", "thì", "mà", "kiểu", "kiểu như", "đại khái",
    "nói chung", "thực ra", "cơ bản là", "nói thật là", "ý là", "tức là",
    "đúng không", "hiểu không", "biết không",
];

/// Strip leading/trailing punctuation and lowercase for matching.
fn normalize(word: &str) -> String {
    word.trim()
        .trim_matches(|c: char| {
            c.is_ascii_punctuation() || matches!(c, '…' | '\u{201C}' | '\u{201D}' | '\u{2019}')
        })
        .to_lowercase()
}

fn is_filler(normalized: &str) -> bool {
    EN_FILLERS.iter().any(|&f| f == normalized) || VI_FILLERS.iter().any(|&f| f == normalized)
}

/// Scan all word timestamps across `segments` and return one `FillerDetection`
/// per filler word (or merged run of adjacent fillers).
///
/// `merge_gap_ms`: filler words whose gap is ≤ this value are merged into a
/// single detection. Pass `0` to disable merging.
pub fn scan_word_timestamps(
    segments: &[TranscriptionSegment],
    merge_gap_ms: i64,
) -> Vec<FillerDetection> {
    let mut detections: Vec<FillerDetection> = Vec::new();

    for (seg_idx, seg) in segments.iter().enumerate() {
        for word in &seg.words {
            let normalized = normalize(&word.text);
            if !is_filler(&normalized) {
                continue;
            }

            // Try merging with the last detection if within merge_gap_ms.
            if merge_gap_ms > 0 {
                if let Some(last) = detections.last_mut() {
                    if word.start_ms - last.cut_end_ms <= merge_gap_ms {
                        last.cut_end_ms = word.end_ms;
                        last.filler_words.push(word.text.trim().to_string());
                        last.text.push(' ');
                        last.text.push_str(word.text.trim());
                        continue;
                    }
                }
            }

            detections.push(FillerDetection::new(
                seg_idx,
                word.start_ms,
                word.end_ms,
                word.text.trim(),
                vec![word.text.trim().to_string()],
            ));
        }
    }

    detections
}

#[cfg(test)]
mod tests {
    use super::*;
    use creator_core::{TranscriptionSegment, WordTimestamp};

    fn seg_with_words(words: &[(&str, i64, i64)]) -> TranscriptionSegment {
        let mut seg = TranscriptionSegment::new(
            words.first().map(|w| w.1).unwrap_or(0),
            words.last().map(|w| w.2).unwrap_or(0),
            words.iter().map(|w| w.0).collect::<Vec<_>>().join(" "),
        );
        seg.words = words
            .iter()
            .map(|&(text, start, end)| WordTimestamp {
                start_ms: start,
                end_ms: end,
                text: text.to_string(),
                confidence: None,
            })
            .collect();
        seg
    }

    #[test]
    fn detects_vietnamese_filler_words() {
        let seg = seg_with_words(&[
            ("Hôm", 0, 200),
            ("ừm", 200, 350),
            ("nay", 400, 600),
            ("tôi", 600, 800),
            ("à", 800, 900),
        ]);
        let detections = scan_word_timestamps(&[seg], 0);
        assert_eq!(detections.len(), 2);
        assert_eq!(detections[0].cut_start_ms, 200);
        assert_eq!(detections[0].cut_end_ms, 350);
        assert_eq!(detections[0].filler_words, vec!["ừm"]);
        assert_eq!(detections[1].cut_start_ms, 800);
    }

    #[test]
    fn merges_adjacent_fillers_within_gap() {
        let seg = seg_with_words(&[
            ("ừm", 0, 200),
            ("à", 250, 400), // 50ms gap → merge with merge_gap_ms=100
        ]);
        let detections = scan_word_timestamps(&[seg], 100);
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].cut_start_ms, 0);
        assert_eq!(detections[0].cut_end_ms, 400);
        assert_eq!(detections[0].filler_words, vec!["ừm", "à"]);
    }

    #[test]
    fn ignores_non_filler_words() {
        let seg = seg_with_words(&[("hello", 0, 300), ("world", 300, 600)]);
        assert!(scan_word_timestamps(&[seg], 0).is_empty());
    }

    #[test]
    fn handles_segments_without_word_timestamps() {
        let seg = TranscriptionSegment::new(0, 1000, "ừm hello ờ");
        // words is empty → nothing to scan
        assert!(scan_word_timestamps(&[seg], 0).is_empty());
    }

    #[test]
    fn strips_punctuation_before_matching() {
        let seg = seg_with_words(&[("ừm,", 0, 300), ("okay", 300, 600)]);
        let detections = scan_word_timestamps(&[seg], 0);
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].cut_start_ms, 0);
    }
}
