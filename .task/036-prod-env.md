# 036 prod in the default envs, the workspace at 0.1.0

status: building · rank: 2 · level: simple · type: infra · request: user, 2026-10-14 ("deploy le learn.palicanon.app") · light path

## Summary
Rank: difficulty 1 (uncomment one block, one version string), impact 2 (adds prod; nothing about dev changes) → 2.
Prod goes in `infra/variables.tf`'s default `envs` (uncomment the `prod` block, `access = false`, host learn.palicanon.app, max 3), not in a git-ignored prod.tfvars: `envs` is replaced as a whole, so an apply run without that file would destroy prod. prod.tfvars.example and infra/README say so. The workspace version goes from 0.0.0 to 0.1.0 (the first prod tag is v0.1.0; Cargo.lock follows).
Then (orchestrator): `tofu apply` (creates learn-pali-prod's secret, SA, IAM, Cloud Run on the bootstrap, the Worker, the Custom Domain), `origin-secret create prod`, `push prod`, `check prod`, then tag v0.1.0 → deploy.yml rolls prod.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | c510918, e4261d0; red: test failed until prod was in variables.tf; green: rollout-check 840/0, tofu validate ok, cargo tests ok |
| Check | done | orchestrator: diff read (prod block, access false; versions 0.0.0 → 0.1.0 only, Cargo.lock 4 lines); rollout-check 840/0 per the lane. No visible result |
