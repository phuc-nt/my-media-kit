//! Region construction: spike removal, grouping, and padding.
//!
//! Operates on a flat `Vec<bool>` of per-frame silent/non-silent flags so
//! each step is O(n) over the frame count (not the sample count).

use creator_core::SilenceRegion;

/// Merge runs of non-silent frames shorter than `spike_seconds` *back into*
/// silence when bordered by silence on both sides. Prevents breath noise
/// and mouth clicks from fragmenting otherwise silent gaps.
pub fn remove_short_spikes(is_silent: &mut [bool], spike_seconds: f64, frame_size_ms: u32) {
    if spike_seconds <= 0.0 {
        return;
    }
    let spike_frames =
        ((spike_seconds * 1000.0) / f64::from(frame_size_ms)).round() as usize;
    if spike_frames == 0 {
        return;
    }

    let n = is_silent.len();
    let mut i = 0;
    while i < n {
        if !is_silent[i] {
            let run_start = i;
            while i < n && !is_silent[i] {
                i += 1;
            }
            let run_end = i; // exclusive
            let run_len = run_end - run_start;
            // Require actual silent neighbors on both sides. If the run sits
            // at the very start or end of the buffer there is no enclosing
            // silence, so we leave it alone rather than destroying edge speech.
            let left_silent = run_start > 0 && is_silent[run_start - 1];
            let right_silent = run_end < n && is_silent[run_end];
            if run_len <= spike_frames && left_silent && right_silent {
                for f in is_silent.iter_mut().take(run_end).skip(run_start) {
                    *f = true;
                }
            }
        } else {
            i += 1;
        }
    }
}

/// Walk the flag buffer and emit `SilenceRegion`s for every run of silent
/// frames that meets the minimum duration.
pub fn build_regions(
    is_silent: &[bool],
    minimum_duration_s: f64,
    frame_size_ms: u32,
) -> Vec<SilenceRegion> {
    let min_ms = (minimum_duration_s * 1000.0).round() as i64;
    let ms_per_frame = i64::from(frame_size_ms);
    let n = is_silent.len();
    let mut out = Vec::new();

    let mut i = 0;
    while i < n {
        if is_silent[i] {
            let start = i;
            while i < n && is_silent[i] {
                i += 1;
            }
            let end = i; // exclusive
            let start_ms = start as i64 * ms_per_frame;
            let end_ms = end as i64 * ms_per_frame;
            if end_ms - start_ms >= min_ms {
                out.push(SilenceRegion::new(start_ms, end_ms));
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Inward padding. A region becomes `[start + padL, end - padR]`; any region
/// that collapses to or below zero duration is dropped.
pub fn apply_padding(
    regions: Vec<SilenceRegion>,
    padding_left_s: f64,
    padding_right_s: f64,
) -> Vec<SilenceRegion> {
    let pad_left = (padding_left_s * 1000.0).round() as i64;
    let pad_right = (padding_right_s * 1000.0).round() as i64;
    regions
        .into_iter()
        .filter_map(|r| {
            let s = r.start_ms + pad_left;
            let e = r.end_ms - pad_right;
            if s < e {
                Some(SilenceRegion::new(s, e))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_spike_between_silence_gets_merged() {
        // silent, silent, loud, silent, silent — with spike_seconds=0.1
        // and 30ms frames, spike limit = 3 frames, so a single-frame spike
        // gets merged.
        let mut flags = vec![true, true, false, true, true];
        remove_short_spikes(&mut flags, 0.1, 30);
        assert_eq!(flags, vec![true, true, true, true, true]);
    }

    #[test]
    fn long_non_silent_run_is_preserved() {
        let mut flags = vec![true, true, false, false, false, false, false, true, true];
        remove_short_spikes(&mut flags, 0.05, 30);
        assert_eq!(
            flags,
            vec![true, true, false, false, false, false, false, true, true]
        );
    }

    #[test]
    fn edge_non_silent_run_not_merged() {
        // Run at the start has no left-silent neighbor, so it must stay.
        let mut flags = vec![false, true, true, true];
        remove_short_spikes(&mut flags, 1.0, 30);
        assert_eq!(flags, vec![false, true, true, true]);
    }

    #[test]
    fn build_regions_groups_runs_above_minimum() {
        // 6 frames silent, 2 loud, 4 silent, with 30ms frames and min 0.1s.
        let flags = vec![
            true, true, true, true, true, true, false, false, true, true, true, true,
        ];
        let r = build_regions(&flags, 0.1, 30);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].start_ms, 0);
        assert_eq!(r[0].end_ms, 180);
        assert_eq!(r[1].start_ms, 240);
        assert_eq!(r[1].end_ms, 360);
    }

    #[test]
    fn build_regions_drops_below_minimum() {
        let flags = vec![true, true, false, true];
        let r = build_regions(&flags, 1.0, 30);
        assert!(r.is_empty());
    }

    #[test]
    fn padding_shrinks_and_drops_collapsed() {
        let regions = vec![
            SilenceRegion::new(1_000, 2_000),
            SilenceRegion::new(5_000, 5_100), // collapses under padding 100+100
        ];
        let padded = apply_padding(regions, 0.1, 0.1);
        assert_eq!(padded.len(), 1);
        assert_eq!(padded[0].start_ms, 1_100);
        assert_eq!(padded[0].end_ms, 1_900);
    }
}
