# 030 deny.toml parses again

status: building · rank: 1 · level: simple · type: fix · request: found by the orchestrator during 029, 2026-10-12 · light path

## Summary
Rank: difficulty 1 (one config file), impact 2 (cargo-deny, a CI job, fails on every run) → 1.
028 (f235e06) inserted `"CC-BY-NC-4.0",` as a stray line inside `[[licenses.exceptions]]`, so cargo-deny no longer reads deny.toml ("expected an equals, found a comma"). The exception itself is obsolete: ui-lib v0.1.0 is CC-BY-NC-4.0, which the allow list holds. Fix: remove the stray line and the whole exception block with its comment; the comment above `allow` still says the crate is MIT OR Apache-2.0 and must stay embeddable in closed-source products, which is false since 028: it says instead that the allow list is permissive licences plus the PaliCanon licence, CC-BY-NC-4.0.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | 4754197; red: `failed to parse config ... expected an equals, found a comma`; green: `advisories ok, bans ok, licenses ok, sources ok` |
| Check | done | orchestrator, tip 7ca9b73: diff read, `cargo deny check` advisories, bans, licenses, sources ok. No visible result |
