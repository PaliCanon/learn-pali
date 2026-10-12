# 032 leptosfmt passes on main again

status: merged · rank: 1 · level: simple · type: style · request: found by lane:guard during 031, 2026-10-12 · light path

## Summary
Rank: difficulty 1 (one generated formatting change in a test), impact 1 (no behaviour; CI's leptosfmt check fails on main) → 1.
`leptosfmt --check .` fails on main in `app/src/components/site_footer.rs` (the test `a_long_page_has_the_footer_after_its_content`: `footer=|| view! { <SiteFooter /> }` wants a block). Since 010 (8c5f268). Fix: run `leptosfmt` on that file, nothing else.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | 82cfeb7; leptosfmt --check 0, fmt 0, footer tests 6 passed |
| Check | done | orchestrator: diff is the generated block only; leptosfmt --check 0 per the lane. No visible result |
| Merge | done | cd65d7b on main (not pushed) |

## Retro
One line (light path): the formatter of view! macros was not run before 010 merged; the merge gate runs every check of the README, leptosfmt included.
