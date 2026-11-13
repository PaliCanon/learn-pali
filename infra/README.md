# infra

learn-pali's own resources, in its own tofu state
(`gs://palicanon-app-tofu-state`, prefix `learn-pali`), inside the GCP
project it shares with the site and the API (`palicanon-app`). Prod runs on
the user's server ssh.lsh.tech (repo `Lsh0x/lsh-tech`, `stacks/palicanon/prod`,
behind a Cloudflare tunnel, the image pulled by digest from GHCR). **Cloud Run
is gone from this directory** (task 083): what is left is what the server
still reads or what is shared.

| What | File | Notes |
| --- | --- | --- |
| Secret `learn-pali-api-token-prod` (`google_secret_manager_secret.api_token["prod"]`) | `gcp.tf` | The bearer the site sends to the PaliCanon API. Tofu creates the secret and **never a version** (the value would sit in its state); `scripts/api-token create prod` adds it. The lsh-tech stack reads it from Secret Manager; no identity of this repository holds a role on it. |
| Drive API (`google_project_service.drive`) | `gcp.tf` | The progress sync calls it from the learner's browser; imported, never disabled on destroy (the PaliCanon app's sync uses it too). |
| `scripts/api-token` | `scripts/` | `create`, `rotate`, `check` the API token (prod only); tested with stub `gcloud`/`security` (`bash infra/scripts/api-token.test.sh`). |

There is one environment, `prod` (the map `envs` in `variables.tf`, host
`learn.palicanon.app`). There is no Cloudflare resource and no provider any
more: the host's DNS record belongs to the lsh-tech tunnel.

The GitHub deploy identity (workload identity federation, `github-deployer`),
the Secret Manager API, the DNS zone and the bucket `palicanon-app-assets`
belong to the site's infra, the API's infra and `platform`: only used here
(`deploy.yml` reads the passages asset with that identity; it pushes the image
with the job's own `GITHUB_TOKEN`).

## Removing Cloud Run (task 083): the order

The Cloud Run service has `deletion_protection = true` in the state, so tofu
cannot destroy it. In order, by the user:

1. Delete the service (this also removes its revisions and its public invoker
   binding's target):

   ```sh
   gcloud run services delete learn-pali-prod --project palicanon-app --region europe-west1
   ```

2. `cd infra && tofu init && tofu plan`, read it, then `tofu apply`. It
   destroys the addresses below; a service already gone is dropped from the
   state by the refresh. `google_secret_manager_secret.api_token["prod"]` and
   `google_project_service.drive` stay.

Destroyed (from the code, no plan run):

- `google_cloud_run_v2_service.app["prod"]` (service `learn-pali-prod`)
- `google_cloud_run_v2_service_iam_member.public["prod"]`
- `google_service_account.app["prod"]` (`learn-pali-prod@…`)
- `google_service_account_iam_member.deployer_acts_as["prod"]`
- `google_secret_manager_secret.origin["prod"]` (`learn-pali-origin-secret-prod`)
- `google_secret_manager_secret_iam_member.app_reads_origin["prod"]`
- `google_secret_manager_secret_iam_member.deployer_reads_origin["prod"]`
- `google_secret_manager_secret_iam_member.app_reads_api_token["prod"]` (the
  Cloud Run runtime account's read of the API token)

The Artifact Registry repository `palicanon` is the site's and the API's (not
managed here); the images of learn-pali in it are no longer written.
