# Docs

Tài liệu của project v2 (Tauri cross-platform rebuild).

## Nội dung

- **[architecture-decisions.md](architecture-decisions.md)** — ADRs giải thích *tại sao* chọn stack / scope / format hiện tại. Đọc đầu tiên khi join project.
- **[llm-tasks.md](llm-tasks.md)** — catalog các feature cần LLM + model options cho mỗi feature. Reference khi config provider hoặc viết prompt mới.
- **[dev-log/](dev-log/)** — nhật ký theo ngày, chỉ ghi việc đã hoàn thành. Format xem `dev-log/README.md`.

## Tài liệu reverse engineering (không check vào repo)

Tài liệu phân tích app gốc `/Applications/My Media Kit` (v1) nằm ở `_research/reverse-engineering/` trên máy local, **ignored trong git** (xem `.gitignore`). Lý do:

- Derived từ binary của app thương mại của bên thứ ba — không phân phối công khai được.
- Chứa prompts + strings extracted; treat như internal research notes.
- Không ảnh hưởng build hay chạy app v2.

Khi cần implement một feature mới, mở file tương ứng trong `_research/reverse-engineering/` để hiểu v1 behavior rồi port sang Rust/TS.

## Khi nào đọc cái gì

| Task | Đọc |
|---|---|
| Onboarding vào project | `architecture-decisions.md` → `dev-log/` mới nhất |
| Implement feature mới | `_research/reverse-engineering/{file}.md` (local) → viết code |
| Config AI provider | `llm-tasks.md` |
| Resume sau khi pause | `dev-log/` (file mới nhất) + git log |
| Tranh luận technical choice | `architecture-decisions.md` |
