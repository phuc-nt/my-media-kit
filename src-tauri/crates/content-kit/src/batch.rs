//! Transcript batching. Groups sequential `TranscriptionSegment`s into
//! duration-limited batches so each AI call fits a sensible token budget.
//!
//! The strategy is simple on purpose: walk segments, accumulate until the
//! wall-clock duration crosses a threshold, emit a batch. No character or
//! token counting — transcripts are verbose enough that duration is a
//! reliable proxy for token count across languages.

use creator_core::TranscriptionSegment;

/// A contiguous slice of the transcript. Holds owned clones so downstream
/// tasks can `spawn` without borrowing.
#[derive(Debug, Clone)]
pub struct TranscriptBatch {
    /// 0-based index in the source transcript; useful for progress UI.
    pub batch_index: usize,
    /// Start index of the first segment in the source transcript.
    pub first_segment_index: usize,
    pub segments: Vec<TranscriptionSegment>,
}

impl TranscriptBatch {
    pub fn duration_ms(&self) -> i64 {
        match (self.segments.first(), self.segments.last()) {
            (Some(first), Some(last)) => last.end_ms - first.start_ms,
            _ => 0,
        }
    }

    /// Flatten the batch into a single prompt-ready transcript string with
    /// `[start_ms - end_ms] text` prefixes. Matches the format v1 used so
    /// the AI gets a stable, unambiguous anchor for each line.
    pub fn to_prompt_transcript(&self) -> String {
        let mut out = String::new();
        for seg in &self.segments {
            out.push_str(&format!(
                "[{} - {}] {}\n",
                seg.start_ms, seg.end_ms, seg.text
            ));
        }
        out
    }
}

/// Chunk a transcript into batches whose wall-clock duration does not
/// exceed `max_duration_s`. A single segment longer than the limit is
/// emitted on its own (we never split a segment mid-text).
pub fn chunk_segments(segments: &[TranscriptionSegment], max_duration_s: f64) -> Vec<TranscriptBatch> {
    let max_ms = (max_duration_s * 1000.0).round() as i64;
    let mut batches = Vec::new();
    let mut current: Vec<TranscriptionSegment> = Vec::new();
    let mut current_start_idx: usize = 0;
    let mut current_start_ms: i64 = 0;

    for (i, seg) in segments.iter().enumerate() {
        if current.is_empty() {
            current_start_idx = i;
            current_start_ms = seg.start_ms;
        }
        let prospective_duration = seg.end_ms - current_start_ms;
        if !current.is_empty() && prospective_duration > max_ms {
            batches.push(TranscriptBatch {
                batch_index: batches.len(),
                first_segment_index: current_start_idx,
                segments: std::mem::take(&mut current),
            });
            current_start_idx = i;
            current_start_ms = seg.start_ms;
        }
        current.push(seg.clone());
    }

    if !current.is_empty() {
        batches.push(TranscriptBatch {
            batch_index: batches.len(),
            first_segment_index: current_start_idx,
            segments: current,
        });
    }

    batches
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(idx: usize, start_ms: i64, end_ms: i64) -> TranscriptionSegment {
        TranscriptionSegment::new(start_ms, end_ms, format!("segment {idx}"))
    }

    #[test]
    fn short_transcript_fits_in_one_batch() {
        let t = vec![seg(0, 0, 5_000), seg(1, 5_000, 10_000)];
        let batches = chunk_segments(&t, 30.0);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].segments.len(), 2);
        assert_eq!(batches[0].first_segment_index, 0);
    }

    #[test]
    fn long_transcript_splits_on_duration() {
        let t = vec![
            seg(0, 0, 10_000),
            seg(1, 10_000, 20_000),
            seg(2, 20_000, 30_000),
            seg(3, 30_000, 40_000),
        ];
        let batches = chunk_segments(&t, 25.0);
        assert!(batches.len() >= 2);
        let seg_count: usize = batches.iter().map(|b| b.segments.len()).sum();
        assert_eq!(seg_count, t.len());
    }

    #[test]
    fn single_segment_longer_than_limit_gets_its_own_batch() {
        let t = vec![seg(0, 0, 120_000)];
        let batches = chunk_segments(&t, 30.0);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].segments.len(), 1);
    }

    #[test]
    fn prompt_transcript_has_timestamp_prefix() {
        let batch = TranscriptBatch {
            batch_index: 0,
            first_segment_index: 0,
            segments: vec![seg(0, 0, 1_000), seg(1, 1_000, 2_000)],
        };
        let text = batch.to_prompt_transcript();
        assert!(text.starts_with("[0 - 1000] segment 0"));
        assert!(text.contains("[1000 - 2000] segment 1"));
    }
}
