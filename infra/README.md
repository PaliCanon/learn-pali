# infra

learn-pali's own resources, in its own tofu state
(`gs://palicanon-app-tofu-state`, prefix `learn-pali`), inside the GCP
project it shares with the site and the API (`palicanon-app`,
`europe-west1`). Per environment (the map `envs` in
`variables.tf`: `dev` first, `prod` added later, see "Order"):

| What | File | Notes |
| --- | --- | --- |
| Runtime service account `learn-pali-<env>` | `gcp.tf` | **One role**: `secretAccessor` on the origin secret, on that secret only. The site reads nothing else from GCP (the deck is embedded, the progress lives in the browser). |
| Secret `learn-pali-origin-secret-<env>` | `gcp.tf` | The value the Worker sends as `x-origin-auth` and the service requires. Tofu creates the secret and **never a version** (the value would sit in its state); `scripts/origin-secret` makes it. |
| The deployer's rights | `gcp.tf` | `github-deployer` is read as a data source (the site's infra owns it), never managed here. It may **act as** the runtime account and **read the origin secret** (see "The deployer reads the origin secret"). |
| Cloud Run service `learn-pali-<env>` | `gcp.tf` | min 0, max 2 (dev) / 3 (prod), 1 CPU, 256Mi, CPU only while a request is served. `ORIGIN_SECRET` comes from the secret (`latest`, read when an instance starts). Startup probe: **TCP 8080** (see "The TCP probe"). Public at the Cloud Run layer. |
| A Worker relaying to Cloud Run | `cloudflare.tf`, `worker/proxy.js` | The API's pattern: the Worker's `ORIGIN` is the Cloud Run URL; it adds `x-origin-auth` from its secret `ORIGIN_AUTH` (overwriting a client's) and answers 500 without it. Tested with `node --test infra/worker/proxy.test.mjs`. |
| Access on dev | `cloudflare.tf` | An Access application on `learn.dev.palicanon.app` and one allow policy on `var.admin_emails`, for the envs with `access = true` (dev). Prod is public until decided otherwise. |
| A Workers Custom Domain per host | `cloudflare.tf` | `learn.palicanon.app` (prod), `learn.dev.palicanon.app` (dev). Cloudflare creates the DNS record and issues the certificate for the exact host, whatever its depth: `learn.dev.palicanon.app` is outside Universal SSL (`*.palicanon.app`), a route on a plain record would not serve it over TLS. For an Access host it `depends_on` the application and its policy: there is no moment when the host is public. |
| `scripts/origin-secret` | `scripts/` | `create`, `push`, `check` the origin secret; tested with stub `gcloud`/`curl` (`bash infra/scripts/origin-secret.test.sh`). |

CI owns the image: tofu ignores `image`, `scaling`, `client` and
`client_version` of the service (`lifecycle.ignore_changes`, the API's rule),
so a deploy never shows up as drift, and CI never touches the environment or
the secrets. The consequence of ignoring `scaling`: a changed `max_instances`
is not applied to an existing service; change it with
`gcloud run services update --max-instances` and keep `variables.tf` in step.

The Artifact Registry repository `palicanon`, the GitHub deploy identity
(workload identity federation), the Secret Manager API and the DNS zone
belong to the site's infra, the API's infra and `platform`: only used here.

## The origin rule

The service answers **403 to any request without `x-origin-auth`** equal to
its `ORIGIN_SECRET` (constant-time comparison; `conventions/deploy.md`), so
the `*.run.app` URL is no way round Cloudflare (its WAF, its per-IP limit,
dev's Access): only the Worker holds the value. The same value lives in two
places, set by `scripts/origin-secret`: Secret Manager (the service reads it)
and the Worker's secret store (the Worker sends it). Neither is in tofu state.

**After any `tofu apply` that re-uploads the Worker, run
`infra/scripts/origin-secret push <env>`.** The Cloudflare provider has no
`keep_bindings`: an apply that changes `worker/proxy.js`, the compatibility
date or `ORIGIN` re-uploads the script, and Cloudflare replaces bindings on
upload unless they are kept. This was **not probed** (it needs a Cloudflare
write). The Worker answers 500 when its secret is missing, so a drop is loud
(`origin-secret check` names `worker-secret`). Declaring the secret in tofu
(`cloudflare_workers_secret`) would put the value in the state: ruled out.

### First deploy: the Worker holds the secret first

The service refuses any request without the value as soon as a revision that
requires it serves, so the Worker must already hold it: in the order of
"Only the user, in order" below, `origin-secret push dev` comes before (or
with) the first `deploy.yml` run. Steps 4 and 5 may swap: CI never needs the
Worker (it checks the revision through its tagged URL with the secret it
reads itself). The bootstrap image does not check the header, so `check`
saying OK before the first real deploy proves nothing about the guard (the
rollout does).

### Rotation (v1)

The value is read when an instance starts (the service) and when
`origin-secret push` runs (the Worker); there is no overlap, so there is a
short window (a few seconds of 403 on dev, behind Access) in which the two
disagree.

1. Add the new version: `openssl rand -hex 32 | gcloud secrets versions add learn-pali-origin-secret-dev --project palicanon-app --data-file=-`
   (the value goes through the pipe, never typed or printed).
2. `infra/scripts/origin-secret push dev` (the Worker now sends the new value).
3. Start a new revision that reads it. `deploy.yml` skips an image that
   already serves, so a rotation is not a CI run: force the revision with
   `gcloud run services update learn-pali-dev --region europe-west1 --revision-suffix rot-<date>`
   (not yet exercised: confirm on the first rotation). Until that revision
   serves, the service answers the Worker with 403.

Prod is not rotated this way: a refusal window on a public site is not
accepted. Prod waits for a two-value rotation (the service accepts the old and
the new value for a while), a follow-up task.

## Order: dev first, prod later

`var.envs` holds **dev only** by default. The first apply therefore stands up
`learn-pali-dev` and `learn.dev.palicanon.app` (behind Access) and nothing
public in prod.

1. **Apply dev** ("Only the user, in order"). The service serves the
   placeholder image; the host asks for a login.
2. **Deploy dev from CI** (push `main`), and check
   `https://learn.dev.palicanon.app`.
3. **Then add prod**: copy `prod.tfvars.example` to `prod.tfvars` (git-ignored)
   and `tofu apply -var-file=prod.tfvars`, or uncomment the `prod` block in
   `variables.tf`. This creates `learn-pali-prod` and `learn.palicanon.app`
   on the placeholder; repeat the secret steps with `prod`
   (`origin-secret create prod`, `push prod`), then the first `v*` tag
   deploys the real image. Prod has its own secret;
   it is public (no Access).

## Only the user, in order (dev)

Nothing here has been applied yet (tasks 021 and 031 wrote it, did not run it).

1. **The Cloudflare token in the keychain** (`lsh` /
   `cloudflare-api-token`, never typed on a command line or printed). One
   token reads and writes the Worker's secret **and** edits Access, accepted
   for dev; scopes: zone DNS, account Workers scripts (Custom Domains
   included), Access applications and policies on the zone.
2. **The ui-lib read credential on learn-pali** — done 2026-10-13: the
   GitHub App `palicanon-ci` (the organisation disables deploy keys),
   repository secrets `APP_ID` and `APP_PRIVATE_KEY`
   (`.github/actions/ui-lib`). Not part of `infra/`.
3. **Log in, then the targeted apply, the secret, the full apply.** The
   Cloud Run service cannot start before the secret has a version, so the
   secret and the accounts come first:

   ```sh
   gcloud auth application-default login
   export CLOUDFLARE_API_TOKEN=$(security find-generic-password -a lsh -s cloudflare-api-token -w)
   export TF_VAR_admin_emails='["<your address>"]'   # who passes Access on dev
   cd infra
   tofu init
   tofu apply \
     -target=google_secret_manager_secret.origin \
     -target=google_service_account.app \
     -target=google_secret_manager_secret_iam_member.app_reads_origin \
     -target=google_secret_manager_secret_iam_member.deployer_reads_origin \
     -target=google_service_account_iam_member.deployer_acts_as
   scripts/origin-secret create dev
   tofu apply
   ```

   The full apply creates Cloud Run on the placeholder image, the Access
   application and its policy, the Worker, then the Custom Domain (after the
   Access resources). It may take a few minutes after the Custom Domain is
   created: Cloudflare issues the host's certificate then.
4. **Give the Worker its secret and check it** (re-run after any apply that
   re-uploads the Worker):

   ```sh
   scripts/origin-secret push dev
   scripts/origin-secret check dev   # prints OK
   ```

   `check` is OK only when a secret version exists, the Worker lists
   `ORIGIN_AUTH` (names only) and `https://learn.dev.palicanon.app/` answers
   a 302 to `*.cloudflareaccess.com`. Otherwise it prints `not OK` and the
   name of the failing check (`version`, `worker-secret`, `access-redirect`).
5. **Push learn-pali `main`**: `deploy.yml` rolls dev. Log in through Access
   and look at https://learn.dev.palicanon.app: the page loads, so the Worker
   relays with the right header.

The services start with the **bootstrap image**
(`us-docker.pkg.dev/cloudrun/container/hello`, `var.bootstrap_image`), which
answers on 8080 and so passes the TCP startup probe.

## What CI's identity must hold

The deploy workflow runs as `github-deployer` (workload identity; the same
account the API's deploy uses, read here as a data source, `gcp.tf`). It must
hold, **on the project** `palicanon-app`:

- `roles/run.developer` (deploy a revision, move traffic, tag),
- Artifact Registry writer on the repository `palicanon` (push the image),
- `roles/iam.serviceAccountUser` on `learn-pali-<env>`: this one **is**
  granted here (`deployer_acts_as`),
- `roles/secretmanager.secretAccessor` on `learn-pali-origin-secret-<env>`,
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
  `learn-pali-origin-secret-<env>`, that secret only. Consequence: any
  repository listed in `github_repos` of the site's infra (workload identity)
  can read that one secret. learn-pali holds no other secret: a bypass would
  spend compute and nothing else.
- **Access on dev, no service token.** CI's check of the public host asserts
  the Access redirect only. An Access service token for CI would put its
  secret in the tofu state, add GitHub secrets and a rotation, and be a second
  way through Access that only the user should have. That the Worker relays is
  shown by the user's own login at step 5 (and `check`'s `worker-secret`).
- **The `Location` rewrite stays as it was** (the incoming origin; the app
  emits no redirect today). search-front's `PUBLIC_ORIGIN` hardening is not
  ported.
