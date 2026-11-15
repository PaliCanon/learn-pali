# 086 — Review cards follow the active theme

status: implemented — local validation passed
rank: 1 (scoped visual correction)
request: User, 2026-11-15, approved ivory cards in light mode and dark blue cards with cream text in dark mode.

## Scope

Remove the fixed light `pc-editorial-paper` palette from Learn's practice
card. Use the existing shared semantic theme tokens for the card, text,
inset fields, buttons and borders. Preserve the reading-sheet shape,
spacing, content and learning behavior. Explicit light/dark preferences
and the system preference must all work, including switching mid-session.
No shared-library change or new theme setting is needed.

## Validation

Formatting, all 66 existing session tests, hydrate Clippy with warnings
denied and the full cargo-leptos build passed against pinned ui-lib
204dd52 through sccache. Browser checks cover Context before/after reveal,
Meaning after grading, desktop/mobile, explicit light/dark and both system
preferences. Theme changes preserve the active session; grading advances
the counter. No JavaScript exceptions or horizontal overflow occurred.

Evidence: `/private/tmp/learn-pali-theme/checks.json`, `qa.json` and the
paired screenshots. Local preview: http://127.0.0.1:4314. No deployment.
