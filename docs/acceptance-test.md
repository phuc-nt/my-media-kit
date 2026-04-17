# Test chấp nhận — Milestone A

15 case kiểm tra UI. Chạy theo thứ tự; skip prereq sẽ fail về sau.

## Chuẩn bị

- Apple Silicon Mac, đã `brew install ffmpeg`, đã `pip install mlx-lm mlx-whisper`
- `npm install` tại repo root
- 3 clip test trong `/tmp/my_media_kit_test/`. Nếu thiếu:
  ```bash
  mkdir -p /tmp/my_media_kit_test
  for src in ~/workspace/creator_util/test-input/*.mp4; do
    name=$(basename "$src" .mp4 | cut -c1-30)
    ffmpeg -y -hide_banner -loglevel error -ss 30 -t 30 -i "$src" \
      -c:v libx264 -preset veryfast -c:a aac /tmp/my_media_kit_test/clip-${name}.mp4
  done
  ```
  Expected: `clip-Hope-…` (JP), `clip-Su-that-…` (VN), `clip-What-Makes-…` (EN).
- Terminal 1: `mlx_lm.server --model mlx-community/Qwen2.5-7B-Instruct-4bit --port 8080 --host 127.0.0.1 --log-level WARNING`
- Terminal 2: `npm run dev`

## Cases

### AT-01 — Launch + layout
**Làm:** đợi cửa sổ mở.
**Pass:** sidebar có title + version + 7 tab, main pane hiện AutoCut + 6 slider + placeholder "pick a source video".
**Fail nếu:** cửa sổ trắng, sidebar thiếu mục, console đỏ.

### AT-02 — Đổi tab
**Làm:** click lần lượt Transcribe → Translate → Summary → Chapters → Export → Settings → AutoCut.
**Pass:** mỗi tab đổi active + main pane đổi theo. Translate/Summary/Chapters hiện "transcribe first". Settings load grid provider.

### AT-03 — Paste source path
**Làm:** paste `/tmp/my_media_kit_test/clip-What-Makes-a-Good-Life-Lessons.mp4` vào ô Source video, Tab để blur.
**Pass:** meta caption đổi từ "no file selected" sang tên file.

### AT-04 — AutoCut lần đầu (ffmpeg extract)
**Làm:** vào tab AutoCut, giữ slider mặc định, click "Detect silence".
**Pass:** status "detecting…" → "N regions" (xanh) trong 1-2s. Bảng regions hiện. Footer ghi `fresh extract`.

### AT-05 — AutoCut re-run (cached)
**Làm:** kéo slider "Min duration" về 0.3, click Detect lần 2.
**Pass:** kết quả trở lại **< 1s**. Footer ghi `cached PCM`.

### AT-06 — Transcribe lần đầu
**Làm:** vào tab Transcribe, giữ mặc định, click "Transcribe".
**Pass:** status "running whisper…" → `N segments` (xanh, không có "(cached)") sau 5-15s. Language: `en`. Bảng segments hiện. Meta caption sidebar thêm `· transcript cached (N segs)`.
**Fail nếu:** lỗi `mlx_whisper: command not found` → `pip install mlx-whisper`.

### AT-07 — Transcribe cache (quay lại tab)
**Làm:** sang tab Summary, quay lại Transcribe.
**Pass:** bảng giữ nguyên, không chạy whisper lại.

### AT-08 — Force re-run
**Làm:** click "Force re-run".
**Pass:** whisper chạy lại, status flip về `N segments` không có "(cached)".

### AT-09 — Translate EN → VI
**Làm:** vào tab Translate, Provider = MLX, target = `vi`, click Translate.
**Pass:** status "translating…" 5-30s → `translated to vi`. Bảng 3 cột Time | Original | Translated. Cột Translated toàn tiếng Việt.
**Fail nếu:** lỗi `provider not registered` → check mlx_lm.server còn chạy.

### AT-10 — Summary
**Làm:** tab Summary, Style = Brief narrative, Language = Vietnamese, click "Run summary".
**Pass:** sau 10-30s hiện 2-3 câu tiếng Việt trong khối tối.
**Fail nếu:** trả về tiếng Anh (ignore language field).

### AT-11 — Chapters + Copy
**Làm:** tab Chapters, Language = Vietnamese, click "Generate chapters". Rồi click "Copy YouTube format", paste vào text editor.
**Pass:** bảng ≥2 chapter, dòng đầu `0:00`, title tiếng Việt. Clipboard chứa dạng `mm:ss title`.
**Fail nếu:** dòng đầu không phải `0:00`.

### AT-12 — VN clip skip rule ⭐
**Làm:** đổi path sang `clip-Su-that-ve-tam-ly-hoc-khong-gi.mp4` → blur. Tab Transcribe → chạy. Tab Translate → click Translate.
**Pass:** Transcribe ra `language: vi`. Translate xong **<1s**, caption `source: vi → target: vi · skipped`, status `skipped (source already matches target)`.
**Fail nếu:** Translate vẫn gọi LLM dù cùng ngôn ngữ → bug `should_skip`.

### AT-13 — JP clip
**Làm:** đổi path sang `clip-Hope-invites-Tsutomu-Uematsu-T.mp4`, Transcribe, Translate.
**Pass:** language = `ja`, Translate ra tiếng Việt, không skip.

### AT-14 — Clear cache
**Làm:** với clip đang cached, click "Clear cache" ở sidebar.
**Pass:** caption đổi thành `cleared cache for <tên file>`. Tab Transcribe hiện "not transcribed yet". Transcribe lại sẽ chạy whisper thật.

### AT-15 — Đổi source giữa các clip
**Làm:** đã Transcribe EN clip, đổi path sang VN clip, blur.
**Pass:** meta caption cập nhật, phần `(N segs)` biến mất. Tab Transcribe hiện "not transcribed yet".

## Giới hạn đã biết (không tính fail)

- Chưa có file dialog — phải paste path thủ công (wire `tauri-plugin-dialog` ở Milestone B).
- Không có progress bar cho whisper / ffmpeg lâu, chỉ có chip "running…".
- Summary caption in enum `Brief` kiểu Debug, chỉ là cosmetic.
- Qwen 3B có thể over-segment trên video dài, mặc định đã lên 7B.
- Filler detection UI chưa wire (command có rồi, chưa có view).
- Tab Export là placeholder, Milestone B.

## Template báo fail

```
Case: AT-XX
Observed:
Expected:
Tauri dev log (40 dòng cuối):
Console (right-click → Inspect → Console):
```
