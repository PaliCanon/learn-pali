# infra

learn-pali's own resources, in its own tofu state
(`gs://palicanon-app-tofu-state`, prefix `learn-pali`), inside the GCP
project it shares with the site and the API (`palicanon-app`,
`europe-west1`). Per environment (the map `envs` in
`variables.tf`: `dev` first, `prod` added later, see "Order"):

| What | File | Notes |
| --- | --- | --- |
| Runtime service account `learn-pali-<env>` | `gcp.tf` | **No role**: the site reads nothing from GCP (the deck is embedded, the progress lives in the browser). |
| The deployer's right to act as it | `gcp.tf` | `github-deployer` is read as a data source (the site's infra owns it), never managed here. |
| Cloud Run service `learn-pali-<env>` | `gcp.tf` | min 0, max 2 (dev) / 3 (prod), 1 CPU, 256Mi, CPU only while a request is served, startup probe on `/`, public. |
| A Worker relaying to Cloud Run | `cloudflare.tf`, `worker/proxy.js` | The API's pattern: the Worker's `ORIGIN` is the Cloud Run URL. |
| A proxied `AAAA 100::` record and the Worker route | `cloudflare.tf` | `learn.palicanon.app` (prod), `learn-dev.palicanon.app` (dev). |

CI owns the image: tofu ignores `image`, `scaling`, `client` and
`client_version` of the service (`lifecycle.ignore_changes`, the API's rule),
so a deploy never shows up as drift. The consequence of ignoring `scaling`: a
changed `max_instances` is not applied to an existing service; change it with
`gcloud run services update --max-instances` and keep `variables.tf` in step.

The Artifact Registry repository `palicanon`, the GitHub deploy identity
(workload identity federation) and the DNS zone belong to the site's infra and
`platform`: only read here.

## Order: dev first, prod later

`var.envs` holds **dev only** by default. The first apply therefore stands up
`learn-pali-dev` and `learn-dev.palicanon.app` and nothing public in prod.

1. **Apply dev** (below). Both the service and the host serve the placeholder
   image.
2. **Deploy dev from CI** (set the repository variables, push `main`), and
   check `https://learn-dev.palicanon.app`.
3. **Then add prod**: copy `prod.tfvars.example` to `prod.tfvars` (git-ignored)
   and `tofu apply -var-file=prod.tfvars`, or uncomment the `prod` block in
   `variables.tf`. This creates `learn-pali-prod` and `learn.palicanon.app`
   on the placeholder; the first `v*` tag deploys the real image.

## First apply

Nothing here has been applied yet (task 021 wrote it, did not run it).

```sh
gcloud auth application-default login
# The Cloudflare token (zone DNS, Workers routes, account Workers scripts)
# comes from the keychain, never typed or printed (conventions/security.md):
export CLOUDFLARE_API_TOKEN=$(security find-generic-password -a palicanon -s cloudflare-api-token -w)
cd infra
tofu init && tofu plan && tofu apply
```

The services start with the **bootstrap image**
(`us-docker.pkg.dev/cloudrun/container/hello`, `var.bootstrap_image`), which
answers `/` on 8080 and so passes the startup probe. There is no secret to
create first, unlike the API's token secret.

## What CI's identity must hold

The deploy workflow runs as `github-deployer` (workload identity; the same
account the API's deploy uses, read here as a data source, `gcp.tf`). It must
hold, **on the project** `palicanon-app`:

- `roles/run.developer` (deploy a revision, move traffic, tag),
- Artifact Registry writer on the repository `palicanon` (push the image),
- `roles/iam.serviceAccountUser` on `learn-pali-<env>`: this one **is**
  granted here (`deployer_acts_as`).

Checked on 2026-10-09 (read-only, `gcloud projects get-iam-policy palicanon-app`):
`github-deployer` holds `roles/artifactregistry.writer`, `roles/run.developer`
and `roles/storage.objectViewer` on the project — the two grants are in place.

**This repository may become the deployer** (2026-10-09): `PaliCanon/learn-pali`
was added to the site's `infra/variables.tf` `github_repos` (`Lsh0x/palicanon`
3091ab1) and applied — the provider `github-oidc` accepts it and
`roles/iam.workloadIdentityUser` on `github-deployer` is granted to it.

Every `gcloud` call of the workflow passes `--project "$GCP_PROJECT"`, so the
project never depends on the runner's default.

## Not done here

- **Cloudflare Access on dev.** The site's dev is behind an Access application
  (Zero Trust team `lucky-cake-cc7c`, allow-list of one address;
  `claude-memory/palicanon-deployment.md`). That application is declared in the
  site's infra, not in the API's, and not here: the policy is per host, so
  `learn-dev.palicanon.app` is public until an Access application is added for
  it (dashboard, or tofu once the token has the Access scope). Decide before
  the first dev deploy.
- **Direct access to the origin (a known gap shared with the API).** The Cloud
  Run service is public (`allUsers` invoker, ingress all) and Cloudflare is in
  front by convention only: anyone who knows the `*.run.app` URL skips the
  WAF, the rate limits and, on dev, Access. The API has the same gap today and
  this service copies it on purpose. A follow-up for both: a secret header the
  Worker adds and the server checks, or restricting ingress. Not fixed here.
- The hosts (`learn.palicanon.app`, `learn-dev.palicanon.app`) are a proposal.
