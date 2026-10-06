# 007 The home tidied: New terms at the right, the data block bare, yellower buttons

status: building · rank: 2 · level: simple · type: feature · path: light · request: learn-pali (user, 2026-10-06) · after: [ui-lib 054]

## Summary
The user, 2026-10-06: « new terme à droite. "Your progress is on this device / It is kept in this browser only…" tu peux le delete, et mettre les boutons à droite. Change la couleur des boutons, plus dans le jaune. »
Decided: 'New terms' always sits at the right (small), also when it is the only action (reverses 003 Fix r7's "only action at the left"), and just above the term list, its picker under it (the user: « new term au-dessus des termes pāli »); the CTA row keeps only Resume / Review the next stage; the data block loses its heading and its sentence (`data_notice`, `data_sentence` removed), Export and Import sit at the right (`justify-end`), 'Not synced across devices yet' goes too (the user: « remove-le aussi ! »; `not_synced` removed); the secondary buttons (New terms, Export, Import) use ui-lib's Accent variant (gold border and tint) instead of Chip — Accent gains its open and pressed states in ui-lib 054. Rank 2: difficulty 1, impact 2.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's words |

## Retro
