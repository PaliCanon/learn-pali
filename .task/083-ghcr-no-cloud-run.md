# 083 Images on GHCR, Cloud Run out

status: merged
rank: 4 — removal across workflow, infra, scripts and docs; no GCP, Cloudflare or GitHub setting touched
level: simple (light path)
request: the user, 2026-11-13: "fini le search, quand tout est bon on vire cloudrun" and, for the registry, "Passer à GHCR … Aller!"
repository: learn-pali

## Summary

Everything PaliCanon serves runs on the user's server ssh.lsh.tech (Lsh0x/lsh-tech,
stacks/palicanon/prod, images pulled by digest). Cloud Run prod gets no traffic.
It is removed from `infra/`, and a tag `v*` now builds the image and pushes it to
`ghcr.io/palicanon/learn-pali` instead of Artifact Registry. Nothing is deployed
from CI: the digest is in the job summary and the stack is updated by hand.

## Design

- `deploy.yml`: one `image` job (`contents: read`, `packages: write`,
  `id-token: write` — GCP only to read the passages asset). `docker/login-action`
  to ghcr.io with `github.actor` / `GITHUB_TOKEN`; tag `h-<same hash>`; skipped
  when `docker buildx imagetools inspect` finds it; label
  `org.opencontainers.image.source`; the release tag is added by `imagetools
  create` whether the image was just built or not; the digest, tags and
  `scripts/release-values.sh` output go to `$GITHUB_STEP_SUMMARY`. Every Cloud Run,
  rollout, Worker, origin-secret and Chrome step is gone.
- `infra/`: Cloud Run service, runtime service account, IAM bindings, public
  invoker, origin secret and its bindings removed; the API token secret
  (`api_token["prod"]`) and the Drive API switch stay. The cloudflare provider,
  `cloudflare.tf`, `bootstrap_image`, `cloudflare_account_id` and the lock entry
  go (no Cloudflare resource is in the state).
- Scripts: `rollout-check.sh` (+ test, Access pattern), `infra/scripts/origin-secret`
  (+ test) and `lifecycle.test.sh` deleted; `release-values.sh` loses its `same`
  mode; the Dockerfile and ui-lib action assertions move to `image.test.sh`; the
  new `deploy-workflow.test.sh` asserts the workflow's shape.
- Code untouched. Dead since this change, kept: the `K_SERVICE` branch (a Cloud Run
  service requires a non-blank `ORIGIN_SECRET`), the `-prod`/`-dev` suffix reading,
  and the origin guard itself (`app/src/origin.rs`; the Dockerfile smoke test
  uses it).

## Resources the next `tofu apply` destroys (after the user deletes the service)

- `google_cloud_run_v2_service.app["prod"]` — service `learn-pali-prod`
  (`deletion_protection`: `gcloud run services delete learn-pali-prod --project palicanon-app --region europe-west1` first)
- `google_cloud_run_v2_service_iam_member.public["prod"]`
- `google_service_account.app["prod"]`
- `google_service_account_iam_member.deployer_acts_as["prod"]`
- `google_secret_manager_secret.origin["prod"]`
- `google_secret_manager_secret_iam_member.app_reads_origin["prod"]`
- `google_secret_manager_secret_iam_member.deployer_reads_origin["prod"]`
- `google_secret_manager_secret_iam_member.app_reads_api_token["prod"]`

Stays: `google_secret_manager_secret.api_token["prod"]` (`learn-pali-api-token-prod`,
read by the server), `google_project_service.drive`.

## Tests

- `scripts/deploy-workflow.test.sh`: red on the old workflow (21 failed), green
  after; mutations (`packages: write` → `read`) caught.
- `scripts/release-values.test.sh`: `same` expected to exit 64, red, then green.
- `scripts/image.test.sh`: the moved Dockerfile / ui-lib assertions; `USER app` →
  `USER root` in a copy is caught.

## Log

| Step | Result |
|---|---|
| Plan | light path; grep of the whole repo; lanes: A (image tests, drop rollout-check), B (release-values), the rest by the lead |
| Build | workflow test red first, then workflow, infra, scripts, docs |
| Verify | see the hand-back's check list |

## Retro

Light path, one line: done from the shared GHCR brief; the first tag is the real test of the new image job.
