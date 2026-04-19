# Hướng dẫn sử dụng My Media Kit

Bộ công cụ AI cho creator: từ một video → transcript, dịch, summary, chapters, YouTube pack, viral clips. Tất cả chạy trên máy bạn (MLX local) hoặc qua OpenAI API (cloud).

---

## 0. Cài đặt OpenAI API key (lần đầu sử dụng)

![Settings — OpenAI key](picture/use-0.png)

App mặc định chạy ở **Cloud mode** (OpenAI), nên trước khi dùng phải nhập API key:

1. Vào tab **Settings** (góc dưới sidebar)
2. Lấy key tại [platform.openai.com/api-keys](https://platform.openai.com/api-keys) (key bắt đầu bằng `sk-...`)
3. Paste vào ô **API key** → bấm **Save key**
4. Key được lưu vào **OS Keychain** (Apple Keychain / Windows Credential Manager) — không nằm trong file plain text

**Updates:** Bấm **Check now** để kiểm tra phiên bản mới của app. Có update sẽ hiện nút Download & Install.

> Chỉ phải làm bước này 1 lần. Lần sau mở app dùng được luôn.

### (Tùy chọn) Cài MLX local — chạy AI offline trên Apple Silicon

Nếu muốn chạy AI hoàn toàn trên máy (không tốn API, không gửi data lên cloud), cần cài thêm 2 Python package:

```bash
pip install mlx-whisper mlx-lm
```

Sau khi cài, dropdown **MLX (local)** sẽ available. Mode này yêu cầu:
- Mac Apple Silicon (M1/M2/M3/M4)
- ≥ 16 GB RAM
- ~9 GB dung lượng để model Qwen3-14B tự download lần đầu

Nếu không cài MLX, dropdown sẽ hiện `MLX (run pip install ...) first` và app vẫn hoạt động bình thường ở Cloud mode.

---

## 1. Mở app & chọn video nguồn

![Giao diện khởi động](picture/use-1.png)

Khi mở app:
- **Mặc định Cloud (OpenAI)** — không tốn RAM, không cần model local
- **AI engine status** (pill bên phải dropdown) sẽ hiện `cloud (OpenAI)` ngay
- **Vietnamese** là ngôn ngữ output mặc định

**Cách nạp video:**
- Bấm **Browse** để chọn file
- Kéo-thả file video vào cửa sổ app
- Paste **YouTube URL** → app tự download (xem section dưới)

Sau khi chọn, app tạo thư mục output `{tên_video}_output/` cạnh file gốc để lưu mọi kết quả.

### ✨ YouTube Downloader tích hợp — điểm sáng

Không cần dùng tool ngoài, không cần cài thêm gì. Paste link YouTube vào ô input là xong:

- **`yt-dlp` đóng gói sẵn** trong app — không phải cài Python, không phải `brew install yt-dlp`
- **Tự đặt tên theo title video**: `~/Downloads/MyMediaKit/12 Angry Men - Not Guilty [0jxVnlRdelU].mp4` — dễ tìm lại sau này
- **Cache thông minh**: paste lại cùng link → bỏ qua download, dùng file đã có
- **Format ổn định**: chọn 360p mp4 muxed sẵn → bỏ qua DASH HD streams thường bị YouTube anti-bot chặn. Đủ chất lượng audio cho transcribe/translate
- **Pipeline liền mạch**: download xong → transcribe → summary → translate → các tính năng khác, tất cả chỉ với 1 link YouTube ban đầu

Đây là khác biệt lớn nhất so với các tool transcription khác — bạn không phải tự download video về trước rồi mới chuyển sang tool khác.

> **Chuyển sang MLX (local):** Bấm dropdown → chọn `MLX (local)`. App sẽ hiện popup xác nhận trước khi load model 9 GB Qwen3-14B. Chỉ dùng khi máy Apple Silicon còn nhiều RAM trống.

---

## 2. Transcribe — chuyển âm thanh thành text

![Transcribe + Summary](picture/use-2.png)

Bấm nút **Transcribe** ở tab đầu tiên. Pipeline:
1. **Whisper** chuyển audio → segments có timestamp (MLX = `whisper-large-v3-turbo`, Cloud = `whisper-1`)
2. Tự động chạy luôn **Summary** ở cột phải — ngay khi transcript xong, model AI sẽ tóm tắt nội dung
3. Tự lưu `transcript.srt`, `transcript.txt`, `summary.md` vào thư mục output

**Status pill:**
- **Transcript** + **Summary** chuyển xanh sau khi xong
- Số segment hiện trên status bar (ví dụ `84 segments + summary`)

**Save clean .srt:** Lọc filler words (ờ, à, um, uh, like…) bằng rule offline — không tốn AI call. Kết quả lưu thành `clean.srt`.

> **Lưu ý:** Summary đã tạo sẵn trong tab Transcribe. Tab **Summary** riêng chỉ dùng khi cần re-summarize với prompt tùy chỉnh (ví dụ "tập trung vào action items").

---

## 3. Translate — dịch transcript sang ngôn ngữ khác

![Translate](picture/use-3.png)

Đặt **ngôn ngữ đích** ở ô bên cạnh dropdown AI engine (mặc định `Vietnamese`), rồi bấm **Translate**.

**Pipeline thông minh:**
- Chia transcript thành nhiều **batch** (mỗi batch ~5 phút)
- Truyền **5 dòng đã dịch trước đó** làm context (đảm bảo nhất quán xuyên suốt)
- Truyền **summary** làm hint (model hiểu chủ đề → dịch tên riêng, thuật ngữ chuẩn hơn)
- Auto-skip nếu `source language == target language`

**Output:** `translate.{Vietnamese}.srt` + `translate.{Vietnamese}.txt`

Bảng kết quả hiện 3 cột: Time / Original / Translated cạnh nhau để bạn so sánh.

---

## 4. Phụ đề trên video

![Subtitles trên video](picture/use-4.png)

Sau khi có file `.srt`, bạn có thể:
- Kéo `translate.Vietnamese.srt` vào player (VLC, IINA, QuickTime, mpv) — phụ đề hiện ngay
- Import vào CapCut, Premiere, DaVinci làm subtitle track
- Upload lên YouTube cùng với video (Studio → Captions)

Format SRT chuẩn nên tương thích mọi tool video editing.

---

## 5. Chapters — markers cho YouTube

![Chapters](picture/use-5.png)

Bấm **Generate chapters** → AI tự phân video thành các chương (5–10 chương/10 phút).

**Quy tắc:**
- Chương đầu **luôn pin về 0:00** (yêu cầu của YouTube)
- Tiêu đề ngắn (≤ 8 từ), mô tả nội dung sắp tới
- Output ngôn ngữ theo setting bên trên

**Copy YouTube format:** Format chuẩn `MM:SS Tên chương` để paste thẳng vào video description trên YouTube Studio.

**Output:** `chapters.json`

---

## 6. YouTube Pack — tiêu đề + mô tả + tags

![YouTube Pack](picture/use-6.png)

Bấm **Generate pack** để có ngay:
- **5 title suggestions** (hook-style, dưới 70 ký tự — phù hợp YouTube)
- **YouTube description** đầy đủ (intro + nội dung + call to action, 150–300 từ)
- **15–20 tags/keywords** SEO

**Tối ưu:** Nếu Summary đã có sẵn, YT Pack dùng summary làm input chính (rất nhanh, vài giây). Nếu chưa có summary, fallback dùng 5 phút đầu transcript.

**Copy all:** Format gọn để paste sang YouTube Studio.

**Output:** `youtube-pack.json`

---

## 7. Viral Clips — tìm khoảnh khắc cho Shorts/Reels/TikTok

![Viral Clips](picture/use-7.png)

Bấm **Find viral clips** → AI quét transcript tìm 3–5 đoạn tốt nhất cho short-form video.

**Mỗi clip có:**
- **Timestamp** chính xác (ms-level) — bạn cắt đúng từ đó trong CapCut/Premiere
- **Hook** — lý do đoạn này engaging (emotional peak, surprising fact, strong opening…)
- **Caption** — gợi ý caption cho social media

Ưu tiên các đoạn 15–60 giây có hook mạnh, có thể standalone không cần context.

**Output:** `viral-clips.json`

---

## 8. Settings — quản lý key & updates

Đã hướng dẫn ở [section 0](#0-cài-đặt-openai-api-key-lần-đầu-sử-dụng). Vào tab **Settings** bất cứ lúc nào để đổi key (Save key đè lên key cũ), xoá key (Delete key), hoặc check phiên bản mới.

---

## Mẹo workflow

**Quy trình tiêu chuẩn cho 1 video:**
1. Transcribe (xong cả Summary luôn)
2. Translate sang ngôn ngữ đích
3. Chapters + YT Pack + Viral Clips (chạy độc lập, song song được)
4. Mở thư mục output → có sẵn 6–7 file để dùng

**Cloud vs Local:**

| | Cloud (OpenAI) | MLX (local) |
|---|---|---|
| Tốc độ | Vài giây/feature | 1–10 phút/feature (tùy độ dài) |
| Chi phí | $$ trả per token | Miễn phí (sau khi tải model 9 GB) |
| Privacy | Gửi qua OpenAI | 100% trên máy bạn |
| RAM | Không tốn | Cần 16 GB+ trống |
| Yêu cầu | API key | Apple Silicon (M1/M2/M3/M4) |

**Khuyến nghị:** Dùng **Cloud** cho công việc hàng ngày (nhanh + ổn định). Switch sang **MLX** khi cần xử lý nội dung nhạy cảm hoặc làm offline.

---

## Output files trong `{video}_output/`

| File | Tính năng |
|------|-----------|
| `transcript.srt` / `.txt` | Transcribe |
| `clean.srt` | Save clean .srt |
| `summary.md` | Summary |
| `translate.{Lang}.srt` / `.txt` | Translate |
| `chapters.json` | Chapters |
| `youtube-pack.json` | YT Pack |
| `viral-clips.json` | Viral Clips |

App tự **scan thư mục output** khi mở lại video — các status badge xanh hiện ngay, click vào tab tương ứng sẽ load lại kết quả từ disk (không phải chạy lại AI).
