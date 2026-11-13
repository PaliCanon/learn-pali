# 082 Drop the dev environment

status: merged
rank: 4 — removal across workflow, infra, scripts and docs; no prod resource touched
level: simple (light path)
request: the user, 2026-11-13: "vire le dev totalement"
repository: learn-pali

## Summary

PaliCanon prod now also runs on the user's server ssh.lsh.tech (Lsh0x/lsh-tech,
stacks/palicanon/prod, images by digest). The dev environment is removed
completely: the Cloud Run `learn-pali-dev` service and everything around it
(service account, secrets, IAM, Worker, Custom Domain `learn.dev.palicanon.app`).
Prod on Cloud Run and the tag path (image build, rollout, checks) stay exactly
as they are; they are removed later, separately.

## Design

- `deploy.yml`: runs on a tag `v*` only (no `push: branches`, no
  `workflow_dispatch`, which only ever deployed dev); `ENV` is `prod`, `HOST`
  `learn.palicanon.app`; `ACCESS_dev` and the dev API URL are gone. A push to
  main runs `ci.yml` only.
- `infra/`: `envs` holds `prod` only; the for_each keys of every prod resource
  are unchanged (`["prod"]`), so no prod address or argument changes. The Worker
  script and Custom Domain, the Access application and policy (never created,
  `access = false`), `admin_emails` and the `access_hosts` output are removed;
  `infra/worker/` (the proxy) is deleted, with its CI step. There is no wrangler
  config in the repo. The cloudflare provider stays declared in `versions.tf`
  (and an emptied `cloudflare.tf` says why): OpenTofu needs it to destroy the
  dev Worker and domain still in the state. A follow-up removes both after that
  apply.
- `infra/scripts/origin-secret` and `api-token` accept `prod` only.
- READMEs describe prod only.

## Resources the next `tofu apply` destroys (from the code, no plan run)

- `google_secret_manager_secret.origin["dev"]`
- `google_secret_manager_secret.api_token["dev"]`
- `google_service_account.app["dev"]`
- `google_service_account_iam_member.deployer_acts_as["dev"]`
- `google_secret_manager_secret_iam_member.app_reads_origin["dev"]`
- `google_secret_manager_secret_iam_member.app_reads_api_token["dev"]`
- `google_secret_manager_secret_iam_member.deployer_reads_origin["dev"]`
- `google_cloud_run_v2_service.app["dev"]`
- `google_cloud_run_v2_service_iam_member.public["dev"]`
- `cloudflare_workers_script.proxy["dev"]`
- `cloudflare_workers_domain.app["dev"]`

No Access application or policy exists in the state (no env set `access`).

## Tests

- `scripts/release-values.test.sh`: deploy.yml wiring (tag trigger only, no
  dispatch, no branch trigger, no dev name, ENV and HOST are prod) — red on the
  old file (5 failed), green after.
- `infra/scripts/origin-secret.test.sh`, `api-token.test.sh`: dev cases moved to
  prod, `dev` refused (red, then green).
- `infra/scripts/lifecycle.test.sh`: the README claims about a branch dispatch
  and a push to main became "no release is written that way"; one mutation
  (re-adding the main claim) is caught.

## Log

| Step | Result |
|---|---|
| Plan | light path, one docs lane (Sonnet), the rest by the lead |
| Build | tests red first, then workflow, infra, scripts, docs |
| Verify | see the hand-back's check list |

## Retro

Light path, one line: dev fixtures hid in script tests the brief did not name; a removal brief should grep the whole repo, tests included, at Plan.
