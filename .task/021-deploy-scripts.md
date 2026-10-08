# 021 The deploy scripts, written but not run

status: building · rank: 5 · level: standard · type: feature · request: learn-pali (user, 2026-10-08: « sans deploy, tu peux faire les scripts de deployment ») · model: the api's deploy (api/.github/workflows/deploy.yml, api/api/Dockerfile, api/infra/) and docs/conventions/deploy.md

## Summary
Write what deploys learn-pali — image, infra, workflow — on the PaliCanon pattern, **without applying, building in CI or pushing anything** (no `tofu apply`, no `gcloud`, no push; the workflow file lands but only runs once the user pushes and sets the repository variables).
- **`app/Dockerfile`** (build context: the parent folder holding `learn-pali/` and `ui-lib/`, as CI checks both out): builder `rust:<the toolchain in rust-toolchain / MSRV ≥ 1.88>-bookworm` + the wasm32 target + `cargo-leptos` (pinned version) + `wasm-bindgen-cli` matching the lockfile; `cargo leptos build --release --locked`; runtime `debian:bookworm-slim`, a non-root user, the server binary and `target/site/`, `LEPTOS_SITE_ADDR=0.0.0.0:8080`, `LEPTOS_SITE_ROOT=site`, `LEPTOS_ENV=PROD`, `PORT=8080`; a `--check`-like smoke (the binary starts and answers `/` with 200 inside the build is not required — say what is possible).
- **`infra/`** (its own tofu state: bucket `palicanon-app-tofu-state`, prefix `learn-pali`; project `palicanon-app`, region `europe-west1`; providers google ~> 6, cloudflare ~> 4.52): a runtime service account per env (no roles: the site reads nothing from GCP), the deployer's act-as on it (deployer read as a data source, like the api), `google_cloud_run_v2_service` `learn-pali-{dev,prod}` (min 0, max 2 dev / 3 prod, cpu 1, 256Mi, `cpu_idle`, startup probe on `/`, image ignored by tofu, CI owns it — same lifecycle as the api), public invoker; Cloudflare: a Worker relaying to Cloud Run (copy the api's `worker/proxy.js` pattern), the proxied `AAAA 100::` record and the route for `learn.palicanon.app` (prod) and `learn-dev.palicanon.app` (dev). `README.md`: what it holds, `tofu init && tofu plan && tofu apply`, the first apply with the bootstrap image, what the user must provide (Cloudflare token in the environment). Cloudflare Access on dev: as the site does today (`claude-memory/palicanon-deployment.md`) — read it and mirror it if the zone's Access app is per host, else note it.
- **`.github/workflows/deploy.yml`**: the api's flow — push to `main` → dev, tag `v*` → prod; workload identity (`vars.WIF_PROVIDER`, `vars.DEPLOYER_SA`, `vars.GCP_PROJECT`); the ui-lib checkout beside it with `UI_LIB_DEPLOY_KEY` (as `ci.yml` does); the image tag `h-<hash of git ls-files -s app core Cargo.toml Cargo.lock + the ui-lib commit>`, built only when missing, pushed to `europe-west1-docker.pkg.dev/<project>/palicanon/learn-pali`; rollout with no traffic under a tag, checks (`/` 200 with 'PaliLearn' in it, one hashed `/pkg/*.wasm` referenced by `/` answers 200, `/review` 404), then traffic. No secrets in the repository.
- **Docs**: README 'Deploy' section; `docs/conventions/deploy.md` unchanged unless a rule is new.
Ruled out: running any of it; a deck fetched at runtime (the deck is embedded). Open for the user: the host names (learn.palicanon.app / learn-dev.palicanon.app proposed), Access on dev, max instances.
Rank 5: difficulty 4 (three artefacts, a two-repo build context, no way to run them here), impact 5 (it will carry the service to production).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's words; the api's deploy read as the model |

## Retro
