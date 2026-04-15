// Shared cut-region state for the AutoCut pipeline.
// Silence, filler, duplicate, and AI-prompt detectors all write here.
// Export panel reads merged keep ranges via buildKeepRanges().
//
// Speech mask: transcript segments act as protected zones.
// Silence cuts that overlap with any speech segment are clipped/removed
// so natural pauses within speech are never cut.
// Filler/duplicate/AI-prompt cuts bypass the mask (they are precise word-level cuts).

const cuts = {
  silence: [],   // [{ start_ms, end_ms }]
  filler: [],    // [{ cut_start_ms, cut_end_ms }]
  duplicate: [], // [{ cut_start_ms, cut_end_ms }]
  aiPrompt: [],  // [{ cut_start_ms, cut_end_ms }]
};

// Speech regions from transcript — protected zones for silence masking.
// Shape: [{ start_ms, end_ms }], sorted ascending.
let speechMask = [];

export function setSilenceCuts(regions) {
  cuts.silence = regions.slice();
}

export function setFillerCuts(detections) {
  cuts.filler = detections.slice();
}

export function setDuplicateCuts(detections) {
  cuts.duplicate = detections.slice();
}

export function setAiPromptCuts(detections) {
  cuts.aiPrompt = detections.slice();
}

// Set transcript speech segments as protected zones for silence masking.
// segments: array of { start_ms, end_ms } (or Whisper segment objects with those fields)
export function setSpeechMask(segments) {
  speechMask = segments
    .map((s) => ({ start_ms: s.start_ms, end_ms: s.end_ms }))
    .sort((a, b) => a.start_ms - b.start_ms);
}

export function clearAll() {
  cuts.silence = [];
  cuts.filler = [];
  cuts.duplicate = [];
  cuts.aiPrompt = [];
  speechMask = [];
}

export function hasCuts() {
  return (
    cuts.silence.length > 0 ||
    cuts.filler.length > 0 ||
    cuts.duplicate.length > 0 ||
    cuts.aiPrompt.length > 0
  );
}

export function getCutCounts() {
  return {
    silence: cuts.silence.length,
    filler: cuts.filler.length,
    duplicate: cuts.duplicate.length,
    aiPrompt: cuts.aiPrompt.length,
  };
}

export function getTotalCutMs() {
  const sumMs = (arr, startKey, endKey) =>
    arr.reduce((s, r) => s + r[endKey] - r[startKey], 0);
  return (
    sumMs(cuts.silence, "start_ms", "end_ms") +
    sumMs(cuts.filler, "cut_start_ms", "cut_end_ms") +
    sumMs(cuts.duplicate, "cut_start_ms", "cut_end_ms") +
    sumMs(cuts.aiPrompt, "cut_start_ms", "cut_end_ms")
  );
}

// Clip a single cut range against the speech mask.
// Returns an array of sub-ranges that don't overlap with any speech segment.
function maskAgainstSpeech(s, e) {
  if (!speechMask.length) return [{ s, e }];

  const result = [];
  let cursor = s;
  for (const sp of speechMask) {
    if (sp.end_ms <= cursor) continue;   // speech ends before cursor, skip
    if (sp.start_ms >= e) break;         // speech starts after cut ends, done

    // Speech overlaps [cursor, e]. Keep the gap before speech.
    if (sp.start_ms > cursor) result.push({ s: cursor, e: sp.start_ms });
    cursor = Math.max(cursor, sp.end_ms);
  }
  // Remaining part after last overlapping speech region
  if (cursor < e) result.push({ s: cursor, e });
  return result;
}

// Merge all cut ranges from every detector, sort, merge overlaps, then invert
// to produce keep ranges ready for export.
// Silence cuts are clipped against the speech mask to avoid cutting mid-speech.
// Filler/duplicate/AI-prompt cuts are applied as-is (they are transcript-derived).
export function buildKeepRanges(totalMs) {
  // Silence cuts: clip against speech mask first
  const silenceCuts = cuts.silence.flatMap((r) =>
    maskAgainstSpeech(r.start_ms, r.end_ms)
  );

  const all = [
    ...silenceCuts,
    ...cuts.filler.map((r) => ({ s: r.cut_start_ms, e: r.cut_end_ms })),
    ...cuts.duplicate.map((r) => ({ s: r.cut_start_ms, e: r.cut_end_ms })),
    ...cuts.aiPrompt.map((r) => ({ s: r.cut_start_ms, e: r.cut_end_ms })),
  ].sort((a, b) => a.s - b.s);

  // Merge overlapping/adjacent cut ranges
  const merged = [];
  for (const c of all) {
    if (merged.length && c.s <= merged[merged.length - 1].e) {
      merged[merged.length - 1].e = Math.max(merged[merged.length - 1].e, c.e);
    } else {
      merged.push({ s: c.s, e: c.e });
    }
  }

  // Invert: gaps between cuts become keep ranges
  const keeps = [];
  let cursor = 0;
  for (const c of merged) {
    if (c.s > cursor) keeps.push({ start_ms: cursor, end_ms: c.s });
    cursor = c.e;
  }
  if (cursor < totalMs) keeps.push({ start_ms: cursor, end_ms: totalMs });
  return keeps;
}
