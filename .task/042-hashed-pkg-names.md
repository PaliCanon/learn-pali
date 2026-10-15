# 042 The wasm and js are served under hashed names

status: design · rank: 4 · level: standard · type: fix · request: ui-lib 063 follow-up d0a3da22bb4254c (2026-10-15)

## Summary
Rank: difficulty 3 (cargo-leptos `hash-files`, the Dockerfile's copy of `pkg/`, the Worker's cache headers, the rollout check's wasm step), impact 5 (a returning visitor loads the old `/pkg/learn-pali.wasm` from cache against the new HTML and panics; seen 2026-10-15 right after the hydration fix) → 4.
`/pkg/learn-pali.{wasm,js,css}` carry no hash and no `Cache-Control`, so browsers cache them heuristically and a deploy serves new HTML with old assets. Use cargo-leptos `hash-files = true` (a `hash.txt` the server reads; files `learn-pali.<hash>.wasm`) or an equivalent, `Cache-Control: public, max-age=31536000, immutable` on hashed assets and `no-cache` on HTML (the Worker or the server), the rollout check following the hashed name it finds in the page. search-front has the same (019).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Found | done | 063's probe, 2026-10-15 |
