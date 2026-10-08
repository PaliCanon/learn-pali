# 020 PaliLearn on a phone

status: review · rank: 1 · level: simple · type: fix · path: light · request: learn-pali (user, 2026-10-08: « sur mobile ce n'est pas affiché, corrige ça »)

## Summary
The brand link (lotus + 'PaliLearn', centred, 1.75rem, a reset of the home since 019) was hidden on a phone (`hidden … sm:flex`), on the idea that the shell's top bar names the site — but the top bar says 'PaliCanon' (ui-lib's wordmark), so on a phone the service's name never shows. It shows at every width now (`flex`); on a phone a little smaller (`text-2xl`, lotus 26 px) with `sm:text-[1.75rem]`, and less space above it. The top bar keeps ui-lib's 'PaliCanon' (the organisation; a per-service wordmark would be an additive ui-lib prop, not asked). Rank 1.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's words |
| Build | done | lane Sonnet: d4ec933 brand shown at every width (text-2xl, sm:text-[1.75rem], lotus 26/30), the column pt-4 sm:pt-6; 305 + 9 tests; clippy, fmt, leptosfmt EXIT 0 |

## Retro
