# 017 PaliLearn bigger

status: merged · rank: 1 · level: simple · type: style · path: light · request: learn-pali (user, 2026-10-08: « mets le PaliLearn en plus gros », with a capture of the home at desktop width)

## Summary
The brand link at the top of the home (lotus + 'PaliLearn', `BrandLink` in home.rs) grows: the name `text-4xl` (was `text-lg`; the user asked twice), the lotus 44 px (was 22), the gap `gap-3`; still a link to `/`, still hidden on a phone (the shell's top bar carries the name there), still left-aligned in the column. Rank 1.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's words (twice) |
| Build | done | lane Haiku: 9480654 BrandLink text-4xl, gap-3, lotus 44 px; 304 tests; clippy, fmt, leptosfmt EXIT 0 |

## Merge
86523a4 (squash, 2026-10-08).

## Retro
One line: the user asked twice before the first build landed; the second ask raised the size mid-lane.
