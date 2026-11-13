# infra

learn-pali's own resources, in its own tofu state
(`gs://palicanon-app-tofu-state`, prefix `learn-pali`), inside the GCP
project it shares with the site and the API (`palicanon-app`,
`europe-west1`). There is one environment, `prod` (the map `envs` in
`variables.tf`, one entry: Cloud Run `learn-pali-prod`, host
`learn.palicanon.app`, max 3). Prod also runs on the user's server
ssh.lsh.tech (repo `Lsh0x/lsh-tech`, `stacks/palicanon/prod`, images by
digest); the Cloud Run prod remains for now and is removed later, separately:

| What | File | Notes |
| --- | --- | --- |
| Runtime service account `learn-pali-prod` | `gcp.tf` | **One role**: `secretAccessor` on the origin secret, on that secret only. The site reads nothing else from GCP (the deck is embedded, the progress lives in the browser). |
| Secret `learn-pali-origin-secret-prod` | `gcp.tf` | The value the service requires as `x-origin-auth`. Tofu creates the secret and **never a version** (the value would sit in its state); `scripts/origin-secret` makes it. |
| The deployer's rights | `gcp.tf` | `github-deployer` is read as a data source (the site's infra owns it), never managed here. It may **act as** the runtime account and **read the origin secret** (see "The deployer reads the origin secret"). |
| Cloud Run service `learn-pali-prod` | `gcp.tf` | min 0, max 3, 1 CPU, 256Mi, CPU only while a request is served. `ORIGIN_SECRET` comes from the secret (`latest`, read when an instance starts). Startup probe: **TCP 8080** (see "The TCP probe"). Public at the Cloud Run layer. |
| `scripts/origin-secret` | `scripts/` | `create` the origin secret (prod only); tested with stub `gcloud`/`curl` (`bash infra/scripts/origin-secret.test.sh`). |

CI owns the image: tofu ignores `image`, the container `env`, `scaling`,
`client` and `client_version` of the service (`lifecycle.ignore_changes`, the
API's rule), so a deploy never shows up as drift, and CI never touches the
secrets. The deploy writes `LEARN_PALI_ENV` and `LEARN_PALI_SHA` into the env
(`gcloud run deploy --update-env-vars`, the version badge reads them), never
tofu. The `env` ignore must be applied (`tofu apply`) before the first deploy
that writes the env, or the next apply strips both variables and the badge
falls back to its local-run value with no diff shown. The consequence of ignoring `env`: a changed
`RUST_LOG` or `ORIGIN_SECRET` reference is not applied to an existing service;
change it with `gcloud run services update` or `tofu apply -replace`. A
`tofu apply -replace` of the service also drops `LEARN_PALI_ENV` and
`LEARN_PALI_SHA`: the badge falls back to its local-run value until the next deploy. A tag release
is restored by re-running the tag's deploy (the deploy runs on a tag only:
re-run the tag's workflow run). The deploy uses `--update-env-vars` and
never `--set-env-vars`: the set variant replaces the whole env and wipes
`RUST_LOG` and the `ORIGIN_SECRET` reference, so every request would be
refused. The consequence of ignoring `scaling`: a changed `max_instances`
is not applied to an existing service; change it with
`gcloud run services update --max-instances` and keep `variables.tf` in step.

There is no Cloudflare resource in this directory any more (no Worker, no
Custom Domain, no Access): the `cloudflare` provider stays declared in
`versions.tf` only so the next `tofu apply` can destroy the Worker and
domain of the removed environment still in the state; a later task removes the provider.

The Artifact Registry repository `palicanon`, the GitHub deploy identity
(workload identity federation), the Secret Manager API and the DNS zone
belong to the site's infra, the API's infra and `platform`: only used here.

## The origin rule

The service answers **403 to any request without `x-origin-auth`** equal to
its `ORIGIN_SECRET` (constant-time comparison; `conventions/deploy.md`), so
the `*.run.app` URL is no way round Cloudflare (its WAF, its per-IP limit).
The value lives in Secret Manager (the service reads it), set by
`scripts/origin-secret create prod`; it is not in tofu state.

The service refuses any request without the value as soon as a revision that
requires it serves. The bootstrap image does not check the header, so the
first real deploy is what proves the guard (the rollout does).

### Rotation (v1)

The value is read when an instance starts; a new version is not seen by a
running instance.

1. Add the new version: `openssl rand -hex 32 | gcloud secrets versions add learn-pali-origin-secret-prod --project palicanon-app --data-file=-`
   (the value goes through the pipe, never typed or printed).
2. Start a new revision that reads it. `deploy.yml` skips an image that
   already serves, so a rotation is not a CI run: force the revision with
   `gcloud run services update learn-pali-prod --region europe-west1 --revision-suffix rot-<date>`
   (not yet exercised: confirm on the first rotation).

A refusal window on a public site is not accepted: prod waits for a two-value
rotation (the service accepts the old and the new value for a while), a
follow-up task. Until then, whoever sends the value must change it at the same
time as step 2.

## Only the user, in order (prod)

The first apply of prod, once. Applied in part on 2026-10-13 (the secret, the
service account, Cloud Run).

1. **The ui-lib read credential on learn-pali**: done 2026-10-13, the
   GitHub App `palicanon-ci` (the organisation disables deploy keys),
   repository secrets `APP_ID` and `APP_PRIVATE_KEY`
   (`.github/actions/ui-lib`). Not part of `infra/`.
2. **Log in, then the targeted apply, the secret, the full apply.** The
   Cloud Run service cannot start before the secret has a version, so the
   secret and the accounts come first:

   ```sh
   gcloud auth application-default login
   cd infra
   tofu init
   tofu apply \
     -target=google_secret_manager_secret.origin \
     -target=google_service_account.app \
     -target=google_secret_manager_secret_iam_member.app_reads_origin \
     -target=google_secret_manager_secret_iam_member.deployer_reads_origin \
     -target=google_service_account_iam_member.deployer_acts_as
   scripts/origin-secret create prod
   tofu apply
   ```

   The full apply creates Cloud Run on the placeholder image. It also
   destroys what the state still holds of the removed environment's Worker and Custom Domain
   (read the plan first: the Cloudflare provider needs `CLOUDFLARE_API_TOKEN`
   from the keychain, `lsh` / `cloudflare-api-token-palicanon`, never typed on
   a command line or printed).
3. **Tag `v0.x.y`**: `deploy.yml` rolls prod.

The services start with the **bootstrap image**
(`us-docker.pkg.dev/cloudrun/container/hello`, `var.bootstrap_image`), which
answers on 8080 and so passes the TCP startup probe.

## What CI's identity must hold

The deploy workflow runs as `github-deployer` (workload identity; the same
account the API's deploy uses, read here as a data source, `gcp.tf`). It must
hold, **on the project** `palicanon-app`:

- `roles/run.developer` (deploy a revision, move traffic, tag),
- Artifact Registry writer on the repository `palicanon` (push the image),
- `roles/iam.serviceAccountUser` on `learn-pali-prod`: this one **is**
  granted here (`deployer_acts_as`),
- `roles/secretmanager.secretAccessor` on `learn-pali-origin-secret-prod`,
  that secret only: granted here (`deployer_reads_origin`).

Checked on 2026-10-09 (read-only, `gcloud projects get-iam-policy palicanon-app`):
`github-deployer` holds `roles/artifactregistry.writer`, `roles/run.developer`
and `roles/storage.objectViewer` on the project.

**This repository may become the deployer** (2026-10-09): `PaliCanon/learn-pali`
was added to the site's `infra/variables.tf` `github_repos` (`Lsh0x/palicanon`
3091ab1) and applied — the provider `github-oidc` accepts it and
`roles/iam.workloadIdentityUser` on `github-deployer` is granted to it.

Every `gcloud` call of the workflow passes `--project "$GCP_PROJECT"`, so the
project never depends on the runner's default.

## Rulings that shape this directory (task 031, ported from search-front 007)

- **The TCP probe.** The service refuses a request without the origin header,
  a Cloud Run HTTP probe header is a plain string in the service resource, and
  the origin secret would then sit in the tofu state and in the revision JSON.
  So the startup probe is TCP on 8080; the HTTP check of `/` runs in the
  rollout, on the tagged revision, before traffic moves, with the header read
  from Secret Manager.
- **The deployer reads the origin secret.** The pre-traffic check needs the
  header, so `github-deployer` has `secretAccessor` on
  `learn-pali-origin-secret-prod`, that secret only. Consequence: any
  repository listed in `github_repos` of the site's infra (workload identity)
  can read that one secret. learn-pali holds no other secret: a bypass would
  spend compute and nothing else.
- **Prod is public behind the origin guard.** No Access: the Cloudflare token
  has no Access permission. CI's step 6b checks the public host
  (`rollout-check.sh --via-worker https://$HOST`): it sends no header, wants
  `/` 200 and the page's `.wasm`, and no token shape in the answers. Exit 1
  (a failed check, after up to 5 tries) puts the traffic back to the previous
  revision; exit 3 (Cloudflare challenged the runner) only warns; exits 4, 6
  and 7 fail the job with no rollback and no retry (the revision is not at
  fault). `ACCESS_prod` in `deploy.yml`'s `env` is false.
- **The `Location` rewrite stays as it was** (the incoming origin; the app
  emits no redirect today). search-front's `PUBLIC_ORIGIN` hardening is not
  ported.
