// Provider → recommended model mapping.
// When the user switches provider, wireProviderModelSync auto-fills the
// model input with a sane default so they don't have to look up IDs.
// MLX is null — each feature keeps its own HTML default (model paths differ).

export const PROVIDER_MODEL_DEFAULTS = {
  mlx: null, // kept as-is per feature HTML value
  claude: "claude-sonnet-4-5-20250929",
  openAi: "gpt-4o-mini",
  gemini: "gemini-2.0-flash",
  ollama: "llama3.2",
  openRouter: "anthropic/claude-3-5-sonnet",
};

// Wire a provider <select> to auto-fill its paired model <input> when the
// selected provider changes. If the new provider has no default (mlx: null),
// the model input is left unchanged so the existing HTML value stays.
export function wireProviderModelSync(selectId, inputId) {
  const sel = document.getElementById(selectId);
  const inp = document.getElementById(inputId);
  if (!sel || !inp) return;
  sel.addEventListener("change", () => {
    const def = PROVIDER_MODEL_DEFAULTS[sel.value];
    if (def) inp.value = def;
  });
}
