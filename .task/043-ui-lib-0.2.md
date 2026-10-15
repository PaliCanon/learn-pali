# 043 learn-pali on ui-lib v0.2.0: the cream light theme

status: merged · rank: 2 · level: simple · type: build · request: ui-lib 065 (user, 2026-10-15: "je préfère la couleur crème du website palicanon"; "merge la lib ui") · light path

## Summary
Rank: difficulty 1 (the ui-lib pin: version "0.1" → "0.2", tag v0.1.2 → v0.2.0, the lockfile), impact 2 (the light theme becomes cream on learn.palicanon.app; no code of learn-pali names a colour) → 2.
ui-lib 0.2.0 (065, 52c050a) replaces the light palette with palicanon.app's cream (a breaking look, so a minor). learn-pali hard-codes no colour (065's grep), so the bump is the whole change. Checked visually on the dev host after the merge (light theme), then prod by a tag.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | b96d796; Cargo.lock: version 0.1.2 → 0.2.0; fmt ✓, clippy ×2 ✓, tests 355 ✓, deny ✓, rollout-check 1024/0 ✓ |
| Check | done | orchestrator: diff read (the pin and one Cargo.lock line); checks per the lane. The visible result (cream) is seen on learn.dev after the merge, before the prod tag |
| Merge | done | 94a0959 on main, pushed (dev deploys) |

## Retro
One line (light path): a bump with no code change; the look is checked on the dev host, where the user sees it before the prod tag.
