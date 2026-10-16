# 046 the rail's Search goes to search.palicanon.app

status: merged · rank: 1 · level: simple · type: fix · request: reported by the vizualisation session, 2026-10-16 (learn-pali app/src/app.rs:94 sends Search to https://palicanon.app, now the showcase www) · light path

## Summary
Rank: difficulty 1 (one href and its tests), impact 2 (a wrong link in the rail on every page) → 1.
The rail's Search entry points at https://palicanon.app (written when search lived there); search lives at https://search.palicanon.app (search-front, prod) and palicanon.app becomes the showcase (www). Fix: the href and the tests that pin it (app.rs lines 215, 225, 244, 481). The shared services list across sites (one source for every rail) is a separate request, designed later.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | d8eea57; red: thread 'app::tests::the_rail_lists_search_then_learn' panicked at app/src/app.rs:216:14 (expected href="https://search.palicanon.app"); green: tests 340, fmt clean, clippy clean |
| Check | done | orchestrator: diff read (one href, four test pins); 340 tests per the lane. Footer links still point at palicanon.app pages (acknowledgements, terms, privacy, support): all 200 today on the old site; www plans legal/support/about only — flagged to the user (www cutover) |
| Merge | done | 90d0919 on main, pushed (dev deploys) |

## Retro
One line (light path): a link written when search lived at palicanon.app outlived the move; the shared services list (designed later) removes the copies.
