# 018 PaliLearn at PaliCanon's size

status: review · rank: 1 · level: simple · type: style · path: light · request: learn-pali (user, 2026-10-08: « plutôt cette taille-là », with a capture of palicanon.app's home brand)

## Summary
The brand takes search-front's home brand exactly (`app/src/components/home.rs` `Brand` in search-front: `flex items-center justify-center gap-3 font-serif text-[1.75rem] font-semibold`, the lotus 30 px), so the two services' names match: `text-[1.75rem]` (was `text-4xl` since 017), the lotus 30 px (was 44), and centred in the column like the reference (Practice and the '+' are centred too). Still a link to `/`, still hidden on a phone. Rank 1.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's capture of palicanon.app; the classes read from search-front |
| Build | done | lane Haiku: 1cf4715 text-[1.75rem], lotus 30, justify-center; 304 tests; clippy, fmt, leptosfmt EXIT 0 |

## Retro
