# 055 A branch can be deployed to dev by the CI

status: merged · rank: 2 · level: simple · type: ci · request: user, 2026-10-17 («learn-pali est pas public ? on peut pas utiliser la CI plutôt ?») · light path

## Summary
Rank: difficulty 2 (a trigger and the env expressions of deploy.yml), impact 2 (branch deploys to dev for user checks, no more laptop deploys) → 2.
deploy.yml runs on a push to main (dev) and on a tag v* (prod). The orchestrator deployed branches to dev from the laptop for the user's checks (050, 054). learn-pali is public, so Actions minutes are free. Add `workflow_dispatch` (no inputs): a manual run deploys the chosen ref to DEV only, never prod (ENV and HOST resolve to dev for a dispatch; the badge shows the branch); the same checks as a main deploy. Usage: `gh workflow run deploy.yml -R PaliCanon/learn-pali --ref <branch>`. The README's deploy section says so; the workflow check script (if the repo has one checking deploy.yml) accepts the new trigger.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | 762bf3f; YAML validation 0 |
| Check | done | orchestrator: a workflow change, nothing visible; the dispatch deploys a branch to dev only (ENV/HOST/concurrency prod only on a tag push); held until 054 left dev |
| Merge | done | aa4354c on main, pushed |

## Retro
One line (light path): a branch deployed to dev is overwritten by the next push to main (rule in deploy.md since 050); this workflow waited for the dev check of 054 to end before merging.
