// Frontend entry. Kept small — each feature view lives in its own module.

import { initSidebar } from "./sidebar.js";
import { initHeader } from "./header.js";
import { initSourcePanel } from "./source-panel.js";
import { initAutoCutView } from "./features/autocut.js";
import { initTranscribeView } from "./features/transcribe.js";
import { initTranslateView } from "./features/translate.js";
import { initSummaryView } from "./features/summary.js";
import { initChaptersView } from "./features/chapters.js";
import { initYouTubePackView } from "./features/youtube-pack.js";
import { initViralClipsView } from "./features/viral-clips.js";
import { initBlogArticleView } from "./features/blog-article.js";
import { initSettingsView } from "./features/settings.js";

window.addEventListener("DOMContentLoaded", () => {
  initSidebar();
  initHeader();
  initSourcePanel();
  initAutoCutView();
  initTranscribeView();
  initTranslateView();
  initSummaryView();
  initChaptersView();
  initYouTubePackView();
  initViralClipsView();
  initBlogArticleView();
  initSettingsView();
});
