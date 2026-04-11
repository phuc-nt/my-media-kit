// Frontend entry. Kept deliberately small — feature views delegate to
// sibling modules in js/features/*.js.

import { initSidebar } from "./sidebar.js";
import { initHeader } from "./header.js";
import { initAutoCutView } from "./features/autocut.js";
import { initSettingsView } from "./features/settings.js";

window.addEventListener("DOMContentLoaded", () => {
  initSidebar();
  initHeader();
  initAutoCutView();
  initSettingsView();
});
