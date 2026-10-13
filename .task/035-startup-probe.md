# 035 the startup probe's timeout is shorter than its period

status: merged · rank: 2 · level: simple · type: fix · request: the first deploy run (38427138169) failed, 2026-10-13 · light path

## Summary
Rank: difficulty 1 (two values in gcp.tf, one message in deploy.yml), impact 2 (no deploy can pass until fixed; nothing served changed: the traffic stayed on the bootstrap) → 2.
The first deploy of learn-pali built its image (h-411c7a1f503285b4) and failed at `gcloud run deploy`: `startup_probe.timeout_seconds: must be less than period_seconds`. infra/gcp.tf has period 2, timeout 3, threshold 30; tofu's v2 API accepted it, gcloud's deploy refuses it. Fix: period 4, timeout 3, threshold 15 (the same 60 s margin). The deploy step's error message guessed "secret has no version?" — it now says to read gcloud's error above and names the usual causes (the probe, the secret's version).
Then: `tofu apply` (the service template), re-run the deploy.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | b59e5e2; red: startup-probe timeout < period, deploy message text (2 assertions); green: rollout-check 839 assertions 0 failed, tofu validate ok, YAML parse ok |
| Check | done | orchestrator: diff read (period 4, timeout 3, threshold 15; the message; two assertions); rollout-check 839/0 per the lane. No visible result |
| Merge | done | 00f5714 on main |

## Retro
One line (light path): tofu's API accepted a probe that gcloud refuses; nothing ran gcloud before the first deploy — a test now pins timeout < period.
