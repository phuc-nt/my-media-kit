// Settings view — provider status grid + key save/delete/refresh.

const { invoke } = window.__TAURI__.core;

async function refreshProviders() {
  const grid = document.getElementById("provider-grid");
  grid.textContent = "Loading…";
  try {
    const list = await invoke("ai_provider_status");
    grid.innerHTML = list
      .map((p) => {
        const reasonLine = p.reason ? `<span class="reason">${p.reason}</span>` : "";
        return `
          <div class="provider-card">
            <span class="dot ${p.available ? "ok" : p.reason ? "err" : ""}"></span>
            <span class="name">${p.displayName}</span>
            ${reasonLine}
          </div>
        `;
      })
      .join("");
  } catch (e) {
    grid.textContent = "failed to load: " + String(e);
  }
}

function setStatus(text, kind = "") {
  const el = document.getElementById("ai-status");
  el.textContent = text;
  el.className = "status " + kind;
}

export function initSettingsView() {
  refreshProviders();

  document.getElementById("btn-refresh-providers").addEventListener("click", () => {
    refreshProviders();
    setStatus("refreshed", "ok");
  });

  document.getElementById("btn-save-key").addEventListener("click", async () => {
    const provider = document.getElementById("ai-provider").value;
    const value = document.getElementById("ai-key").value;
    if (!value.trim()) {
      setStatus("enter a key first", "err");
      return;
    }
    try {
      await invoke("ai_set_api_key", { provider, value });
      setStatus("saved", "ok");
      document.getElementById("ai-key").value = "";
      refreshProviders();
    } catch (e) {
      setStatus(String(e), "err");
    }
  });

  document
    .getElementById("btn-delete-key")
    .addEventListener("click", async () => {
      const provider = document.getElementById("ai-provider").value;
      try {
        await invoke("ai_delete_api_key", { provider });
        setStatus("deleted", "ok");
        refreshProviders();
      } catch (e) {
        setStatus(String(e), "err");
      }
    });
}
