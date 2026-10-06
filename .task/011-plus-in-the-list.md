# 011 The + inside the term list, bottom right; export and import in Settings

status: planned · rank: 3 · level: simple · type: feature · path: light · request: learn-pali (user, 2026-10-06: « j'aimerais que le bouton plus soit dans la liste en bas à droite ») · after: [010, ui-lib 057]

## Summary
The '+' (Accent, opens the New terms dialog) leaves the CTA row and sits inside the term list's scroll region, at its bottom right, sticky (`sticky bottom-2` in a right-aligned row after the `<ul>`, `ml-auto`, a small shadow so it reads above the cards), so it stays in reach while the list scrolls; the CTA row keeps Practice, the slider and 'Next review'. When no term is started (no list), the '+' sits where the list would be, right-aligned. Mobile: the same, the list's region is full width.
The user's second remark on the same capture (« ces infos, mets-les dans la card pāli ») is already built (008: the card holds its detail); the capture showed the build before 008 from the browser's cache — 010 fixes the cache.
**Export and import move to Settings** (the user: « import export dans les settings »): the home's data block goes; Settings gains a section 'Your progress' (title key; fr 'Votre progression', vi 'Tiến độ của bạn') after Appearance, through ui-lib 057's `settings_extra`, holding Export and Import (Accent) and the import's confirmation (the import Dialog replaces Settings while it confirms — the one-open rule; Replace or Cancel). The broken states keep their own way out on the home: Invalid → 'Download the raw copy' and 'Start again', and Import now lives in Settings (the banner says so: 'or import a copy in Settings'); NewerVersion → the raw copy only, and Settings' Import is disabled with the reason.
Rank 2 → 3 (a cross-repo dependency, the broken-state wording).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's words |

## Retro
