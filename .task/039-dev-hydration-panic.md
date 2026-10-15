# 039 learn.dev.palicanon.app panics at hydration

status: merged (by 040) · rank: 5 · level: standard · type: bug · request: found by www 004's lead, 2026-10-14 · process: ../../docs/workflows/process-bug.mmd

## Summary
Rank: difficulty 4 (a hydration panic inside tachys, a Cloudflare injection suspected, to reproduce and pin), impact 6 (the dev site is unusable in a browser: stuck on "Loading your progress…") → 5.
Reproduced (www 004 lead, headless Chrome through CDP, 390×844, light, 24 s): the page stays on "Loading your progress…"; the console shows `panicked at tachys-0.2.19/src/hydration.rs:227:9: internal error: entered unreachable code`, then an uncaught wasm error. The DOM holds an `<a href="https://learn.dev.palicanon.app/cdn-cgi/content?id=…">` the server never rendered. Hypothesis (unchecked): Cloudflare injects markup into the HTML (an email-obfuscation or bot link), so hydration meets a node it did not render. The Cloudflare Insights beacon is also blocked (integrity mismatch), so a second injection (the beacon script) may be on.
To do (process-bug): reproduce locally against the dev origin with and without the Cloudflare layer (the run.app URL with the origin header); list the zone's injecting features (Email Obfuscation, Web Analytics / RUM, Bot Fight) and which one adds that node; the fix is at the zone or the Worker (disable the injection for this host) or in the shell (hydration tolerant of foreign nodes); a regression test; the prod host checked the same way before any tag.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Found | done | www 004 lead, 2026-10-14; the www learn scene falls back to its drawing (www 004 ruling) |
| Fix | fixed by 040 | the cause is ui-lib 063's (the Cloudflare decoy link first in `<body>`); learn-pali 040 hydrates from ui-lib v0.1.2's root and checks hydration in headless Chrome on the public host (branch fix/hydration-root) |
| Fixed | done | by 040 (ui-lib 063): learn.palicanon.app and learn.dev hydrate again, 2026-10-15. |
