# 004 The deck pipeline: curated order, the "why now" line, French and Vietnamese, the example's translation

status: design · rank: — (ranked at Define) · level: — · type: feature · request: learn-pali site UX (session learn-pali-003-site-ux, design v3) · after: [003]

Not started: defined with the user next.

## Summary (scope gathered so far)
- **Curated order** (planned since 003): the deck's terms ordered for reading the Canon — Buddhist terms and the words that help read the suttas first (DPD `family_set`, corpus frequency `ebt_count`), not alphabetical.
- **The "why now" line**: the first card of a batch says why this term comes now (a family, a frequent particle, a sutta it unlocks).
- **French and Vietnamese glosses** (docs/projects/learn-pali/concepts.md, "The deck's languages"): the DPD is English only; the Tipiṭaka app's copy has French for 922 of 61,596 senses (1.5 %, machine-like); glosses to produce, reviewed, cached in the deck; English shown until one exists.
- **The example line's translation** (user, 2026-10-07, on a Context card of Dhp 127 « na antalikkhe na samuddamajjhe… »: « ce serait bien d'avoir la traduction »): the pipeline resolves each example's `source` (DPD's `DHP127`, `MN10`…) to the SuttaCentral uid and segment ids (Dhp 127 sits in `dhp116-128`), fetches the line's translation from the PaliCanon API (`GET /v1/sutta/{uid}/{lang}[/{translator}]`, segment by segment, token-gated: the pipeline runs with the build's token, never the browser) in en, fr, vi where a translator exists, and stores it in the deck with the translator's name. The reveal shows it under the Pāli line in the selected language, English as the fallback, the translator credited; a Context card's front stays Pāli only (the translation would give the answer away).
- **Open for Define**: the deck's size; the order's signals and weights; who reviews the generated glosses; whether a translation that does not exist in fr/vi falls back to English or shows nothing; the deck format version (v2) and the app's migration.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | todo | scope gathered (003, the user's request of 2026-10-07) |

## Retro
