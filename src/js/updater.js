// Auto-updater module for Tauri v2.
// Checks GitHub releases via tauri-plugin-updater.
// Tries plugin API first, then invoke fallback.

class Updater {
  constructor() {
    this.updateAvailable = null;
    this.onUpdateFound = null;   // (version, notes) => void
    this.onCheckComplete = null; // (hasUpdate) => void
    this.onError = null;         // (error) => void
  }

  async checkForUpdates() {
    const check = window.__TAURI__?.updater?.check;
    if (check) return this._checkViaPlugin(check);

    const invoke = window.__TAURI__?.core?.invoke;
    if (invoke) return this._checkViaInvoke(invoke);

    console.warn("[Updater] No updater API found");
    if (this.onCheckComplete) this.onCheckComplete(false);
  }

  async _checkViaPlugin(check) {
    try {
      const update = await check();
      if (update) {
        this.updateAvailable = update;
        if (this.onUpdateFound) this.onUpdateFound(update.version, update.body || "");
        if (this.onCheckComplete) this.onCheckComplete(true);
      } else {
        if (this.onCheckComplete) this.onCheckComplete(false);
      }
    } catch (err) {
      console.warn("[Updater] check failed:", err);
      if (this.onError) this.onError(err);
      if (this.onCheckComplete) this.onCheckComplete(false);
    }
  }

  async _checkViaInvoke(invoke) {
    try {
      const result = await invoke("plugin:updater|check");
      if (result?.available) {
        this.updateAvailable = result;
        if (this.onUpdateFound) this.onUpdateFound(result.version, result.body || "");
        if (this.onCheckComplete) this.onCheckComplete(true);
      } else {
        if (this.onCheckComplete) this.onCheckComplete(false);
      }
    } catch (err) {
      console.warn("[Updater] invoke check failed:", err);
      if (this.onError) this.onError(err);
      if (this.onCheckComplete) this.onCheckComplete(false);
    }
  }

  async downloadAndInstall(onProgress) {
    if (!this.updateAvailable) return;
    try {
      if (typeof this.updateAvailable.downloadAndInstall === "function") {
        let downloaded = 0;
        let total = 0;
        await this.updateAvailable.downloadAndInstall((event) => {
          if (event.event === "Started") total = event.data.contentLength || 0;
          if (event.event === "Progress") {
            downloaded += event.data.chunkLength;
            if (onProgress) onProgress(downloaded, total);
          }
        });
      } else {
        const invoke = window.__TAURI__?.core?.invoke;
        if (invoke) await invoke("plugin:updater|download_and_install");
      }
    } catch (err) {
      console.error("[Updater] install failed:", err);
      throw err;
    }
  }
}

export const updater = new Updater();
