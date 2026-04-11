// Tiny reactive store shared across feature views. Holds the currently
// selected source path + last known transcript snapshot so each tab can
// read without re-invoking whisper. Emits `change` events so views can
// refresh their "no source picked" placeholders when the user types in
// the sidebar input.

const state = {
  path: "",
  transcript: null, // { language, segments }
};

const listeners = new Set();

function notify() {
  for (const fn of listeners) {
    try {
      fn({ ...state });
    } catch (err) {
      console.error("source-store listener failed:", err);
    }
  }
}

export function getSource() {
  return { ...state };
}

export function setSourcePath(path) {
  const trimmed = (path || "").trim();
  if (trimmed === state.path) return;
  state.path = trimmed;
  // Drop cached transcript whenever the source changes — downstream views
  // should fall back to whatever the backend has cached.
  state.transcript = null;
  notify();
}

export function setTranscript(transcript) {
  state.transcript = transcript
    ? {
        language: transcript.language ?? null,
        segments: transcript.segments ?? [],
      }
    : null;
  notify();
}

export function subscribe(fn) {
  listeners.add(fn);
  fn({ ...state });
  return () => listeners.delete(fn);
}
