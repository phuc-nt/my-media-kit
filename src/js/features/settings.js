// Settings view — OpenAI API key management + auto-updater UI.

import { updater } from "../updater.js";

const { invoke } = window.__TAURI__.core;

async function refreshOpenAiStatus() {
  const card = document.getElementById("openai-key-status");
  try {
    const list = await invoke("ai_provider_status");
    const openai = list.find((p) => p.id === "openAi" || p.displayName?.toLowerCase().includes("openai"));
    if (!openai) { card.innerHTML = `<span class="dot"></span> OpenAI`; return; }
    const dot = openai.available ? "ok" : openai.reason ? "err" : "";
    const reason = openai.reason ? ` — ${openai.reason}` : "";
    card.innerHTML = `<span class="dot ${dot}"></span> OpenAI${reason}`;
  } catch (e) {
    card.textContent = "status check failed: " + String(e);
  }
}

function setStatus(text, kind = "") {
  const el = document.getElementById("ai-status");
  el.textContent = text;
  el.className = "status " + kind;
}

export function initSettingsView() {
  // Lazy-load: only check Keychain when Settings tab is first shown
  // to avoid macOS Keychain popup on every app launch during dev.
  let providerChecked = false;
  const settingsView = document.querySelector('[data-view="settings"]');
  const observer = new MutationObserver(() => {
    if (!providerChecked && settingsView.classList.contains("active")) {
      providerChecked = true;
      refreshOpenAiStatus();
    }
  });
  observer.observe(settingsView, { attributes: true, attributeFilter: ["class"] });

  // ── API key management ──────────────────────────────────────────────
  document.getElementById("btn-save-key").addEventListener("click", async () => {
    const value = document.getElementById("ai-key").value;
    if (!value.trim()) { setStatus("enter a key first", "err"); return; }
    try {
      await invoke("ai_set_api_key", { provider: "openAi", value });
      setStatus("saved", "ok");
      document.getElementById("ai-key").value = "";
      refreshOpenAiStatus();
    } catch (e) { setStatus(String(e), "err"); }
  });

  document.getElementById("btn-delete-key").addEventListener("click", async () => {
    try {
      await invoke("ai_delete_api_key", { provider: "openAi" });
      setStatus("deleted", "ok");
      refreshOpenAiStatus();
    } catch (e) { setStatus(String(e), "err"); }
  });

  // ── Auto-updater ────────────────────────────────────────────────────
  const statusText     = document.getElementById("update-status-text");
  const btnCheck       = document.getElementById("btn-check-update");
  const availableBox   = document.getElementById("update-available");
  const versionEl      = document.getElementById("update-version");
  const notesEl        = document.getElementById("update-notes");
  const btnDoUpdate    = document.getElementById("btn-do-update");
  const progressBox    = document.getElementById("update-progress");
  const progressBar    = document.getElementById("update-progress-bar");
  const progressPct    = document.getElementById("update-progress-pct");

  updater.onUpdateFound = (version, notes) => {
    statusText.textContent = `Update available: v${version}`;
    versionEl.textContent = `v${version}`;
    notesEl.textContent = notes || "";
    availableBox.hidden = false;
  };

  updater.onCheckComplete = (hasUpdate) => {
    if (!hasUpdate) statusText.textContent = "You're on the latest version";
    btnCheck.disabled = false;
  };

  updater.onError = () => {
    statusText.textContent = "Update check failed — try again later";
    btnCheck.disabled = false;
  };

  btnCheck.addEventListener("click", () => {
    statusText.textContent = "Checking for updates…";
    btnCheck.disabled = true;
    availableBox.hidden = true;
    updater.checkForUpdates();
  });

  btnDoUpdate.addEventListener("click", async () => {
    btnDoUpdate.disabled = true;
    progressBox.hidden = false;
    progressBar.style.width = "0%";
    progressPct.textContent = "0%";

    try {
      await updater.downloadAndInstall((downloaded, total) => {
        if (total > 0) {
          const pct = Math.round((downloaded / total) * 100);
          progressBar.style.width = `${pct}%`;
          progressPct.textContent = `${pct}%`;
        }
      });
      statusText.textContent = "Update installed — restarting…";
      // Attempt restart.
      try {
        const relaunch = window.__TAURI__?.process?.relaunch;
        if (relaunch) await relaunch();
      } catch (_) {
        statusText.textContent = "Update installed — please restart the app manually";
      }
    } catch (e) {
      statusText.textContent = `Update failed: ${e}`;
      btnDoUpdate.disabled = false;
      progressBox.hidden = true;
    }
  });

  // Auto-check updates 5 seconds after init (doesn't touch Keychain).
  setTimeout(() => updater.checkForUpdates(), 5000);
}
