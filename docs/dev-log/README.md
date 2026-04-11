# Dev Log

Nhật ký **what was done** theo ngày của CreatorUtils v2.

## Quy tắc

- Một file per ngày: `YYYY-MM-DD.md`. Nhiều phiên cùng ngày gộp vào cùng file.
- Chỉ ghi **việc đã xong** + artifact (file path, test count, commit). Không ghi plan, không ghi ý định.
- Mỗi entry ≤ 20 dòng, liệt kê theo bullet.
- Test/metric số cụ thể được ưu tiên hơn mô tả.
- Không duplicate nội dung đã có trong `plans/reports/` — chỉ link tới.

## Template

```markdown
# YYYY-MM-DD

## Shipped
- <item 1> · <file or test count>
- <item 2>

## Changed
- <what changed, why>

## Fixed
- <bug>: <root cause, fix location>

## Verified
- <test suite> · <pass count>
- Related: plans/reports/<report>.md

## Known follow-ups
- <thing not done but discovered>
```

Sections được phép bỏ nếu không có nội dung.

## Index

- [2026-04-10.md](2026-04-10.md) — Tauri pivot + 7 kits scaffolded + 105 unit tests
- [2026-04-11.md](2026-04-11.md) — Real media pipeline verified + docs restructure + MLX-first + Translate feature
