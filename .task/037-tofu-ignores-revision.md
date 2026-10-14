# 037 tofu ignores the revision name the deploy sets

status: merged · rank: 1 · level: simple · type: fix · request: found by the orchestrator during the prod apply, 2026-10-14 · light path

## Summary
Rank: difficulty 1 (one entry in a lifecycle block), impact 2 (every apply after a deploy creates a useless Cloud Run revision; traffic untouched) → 1.
`gcloud run deploy` (deploy.yml) names each revision; tofu then plans `template.revision = "<name>" -> null` on the service and, applied, creates a new revision with the same image and no traffic (happened to learn-pali-dev on 2026-10-14: revision 00004). Fix: add `template[0].revision` to the service's `lifecycle.ignore_changes`, beside `image`, with a comment; a static test pins it.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | 93604da; red: assert ignore_changes holds template[0].revision; green: rollout-check 841/0, tofu fmt ok, tofu validate ok | |
| Check | done | orchestrator: diff read; rollout-check 841/0 per the lane; after merge, `tofu plan` must show no change for either service |
| Merge | done | 16bae1e on main; `tofu plan` after it: "No changes. Your infrastructure matches the configuration." |

## Retro
One line (light path): the drift came from two writers on one resource (tofu, the deploy); the field the deploy owns is now ignored by tofu, as the image already was.
