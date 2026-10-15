# 041 The Dockerfile smoke waits a fixed time

status: design · rank: 2 · level: simple · type: bug · light path · request: found during 040 (2026-10-15)

## Summary
Rank: difficulty 1 (a wait loop instead of a fixed sleep), impact 2 (a red image build on a busy machine or runner) → 2.
`app/Dockerfile:121-123`: the smoke test waits a fixed time before hitting 8080; on a busy macOS (a cargo build running) it failed 2 of 4 runs ("smoke: no answer on 8080") and passed idle (040's lane). Replace the fixed wait by a bounded poll (up to 60 s, every second), as search-front's smoke.sh does; a test of the poll with a slow fake server.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Found | done | 040's lane, 2026-10-15 |
