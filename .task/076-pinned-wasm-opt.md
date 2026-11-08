# 076 Preinstall the pinned frontend build tools

status: publishing · rank: 2 · level: simple · type: fix · request: production publication of 075, user 2026-11-08 · light path · after: [075]

## Summary

The user explicitly requested publication of 075 before the next vocabulary
pack. The dev image build on main (`42468697272`) compiled the optimized
WASM successfully, then cargo-leptos failed to download Binaryen 123 from
GitHub's release-page URL. That exact URL also returns 504 from this Mac.
The official GitHub asset API supplies the same archive successfully.

Difficulty 2 (one Docker tools layer), impact 2 (build delivery only) → 2.
No application code, optimizer version, release flags or progress format
changes. The production release remains v0.1.20 until the new image passes
its normal rollout checks.

## Fix

Preinstall `wasm-opt` in the Docker tools stage from official asset
240710285 (Binaryen version_123, x86_64 Linux), with bounded retries and
its published SHA-256. Extract only the executable to PATH and check its
version. cargo-leptos 0.3.11 checks PATH first, so it uses that executable
and keeps release optimization enabled. Keep this in the cached tools
stage so source changes do not fetch it again.

```mermaid
flowchart LR
    A[Official Binaryen 123 asset API] --> B[Verify published SHA-256]
    B --> C[Install wasm-opt on PATH and check version]
    C --> D[cargo-leptos finds installed executable]
    D --> E[Normal optimized release build and guarded rollout]
```

## Evidence and validation

- Official metadata: WebAssembly/binaryen release version_123; checksum
  asset 240710284. Downloaded archive matches published SHA-256
  `e959f2170af4c20c552e9de3a0253704d6a9d2766e8fdb88e4d6ac4bae9388fe`.
- Verified the archive contains `binaryen-version_123/bin/wasm-opt` and
  cargo-leptos v0.3.11 `Exe::get` uses PATH before its downloader.
- Application source is identical to 075, whose full application CI passed.
- Full container build and normal revision/public-browser rollout checks
  validate the Linux executable and the image in GitHub Actions.
- Infrastructure files are unchanged. Their CI installer also gets 504;
  equivalent checks passed locally with cached pinned providers (075 PR).

## Log

| Step | Result |
| --- | --- |
| Define | Necessary build repair under the user's explicit production-publication instruction; same existing tool, version and output contract. |
| Verify | Local checksum/archive and upstream downloader checks passed; Docker tools layer verifies checksum and executable version before compilation. |

## Retro

A release build should cache and verify its large pinned build tools before
compiling application sources, rather than download them after compilation.

## Tailwind follow-up

The next build (`42469442927`) verified and ran Binaryen successfully,
then hit the same release-page download failure for Tailwind v4.2.1.
Preinstall that existing tool via official asset API id 360733128 as well,
using the release metadata's published digest
`39e8d4e24b3c83b0a6e69e100a972fbc75d5fef8dce47b3ddac3cf92dea81fe3`.
The cached tools layer checks that digest, marks the standalone executable
executable and runs its help before compiling sources. cargo-leptos uses
the same PATH-first lookup for Tailwind. Application source is unchanged.
