// Sidebar header — app version + platform info footer.

const { invoke } = window.__TAURI__.core;

export async function initHeader() {
  const versionEl = document.getElementById("app-version");
  if (versionEl) {
    try {
      versionEl.textContent = "v" + (await invoke("app_version"));
    } catch {
      versionEl.textContent = "v?";
    }
  }

  const footer = document.getElementById("platform-info");
  if (footer) {
    try {
      const info = await invoke("platform_info");
      const bits = [info.os, info.arch];
      if (info.supportsMlx) bits.push("MLX");
      if (info.supportsAppleIntelligence) bits.push("AI-capable");
      footer.textContent = bits.join(" · ");
    } catch {
      footer.textContent = "platform unknown";
    }
  }
}
