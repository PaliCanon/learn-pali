# Shared with the site's infra (palisearch): read, never managed here.
data "google_service_account" "deployer" {
  account_id = "github-deployer"
}

# ---------------------------------------------------------------- service account

# The identity the service runs as. It has no role on purpose: the site
# reads nothing from GCP (the deck is embedded in the binary, the progress
# lives in the visitor's browser). A role added here is a decision to write
# down, not a default.
resource "google_service_account" "app" {
  for_each     = var.envs
  account_id   = "learn-pali-${each.key}"
  display_name = "learn-pali (${each.key})"
}

# `gcloud run deploy` in CI acts as the runtime service account.
resource "google_service_account_iam_member" "deployer_acts_as" {
  for_each           = var.envs
  service_account_id = google_service_account.app[each.key].name
  role               = "roles/iam.serviceAccountUser"
  member             = "serviceAccount:${data.google_service_account.deployer.email}"
}

# ---------------------------------------------------------------- the site

resource "google_cloud_run_v2_service" "app" {
  for_each = var.envs
  name     = "learn-pali-${each.key}"
  location = var.region
  ingress  = "INGRESS_TRAFFIC_ALL"

  template {
    service_account = google_service_account.app[each.key].email
    scaling {
      min_instance_count = 0
      max_instance_count = each.value.max_instances
    }
    timeout = "60s" # server-rendered pages, no streaming

    containers {
      image = var.bootstrap_image
      ports {
        container_port = 8080
      }
      resources {
        limits = {
          cpu    = "1"
          memory = "256Mi"
        }
        # CPU only while a request is served: scale to zero costs nothing.
        cpu_idle = true
      }
      # The Leptos settings (site address and root, hashed files, PROD)
      # are the image's own ENV (app/Dockerfile), so they cannot drift
      # from the files they describe. RUST_LOG is the only knob here.
      env {
        name  = "RUST_LOG"
        value = "info"
      }
      startup_probe {
        http_get {
          # `/` is a real page: it answering 200 means the server renders.
          # (Cloud Run's front end never lets /healthz reach a container.)
          path = "/"
        }
        # The first render happens on the probe; the margin covers a slow
        # image pull on a cold start.
        period_seconds    = 2
        timeout_seconds   = 3
        failure_threshold = 30
      }
    }
  }

  lifecycle {
    # CI deploys images; tofu owns everything else (same rule as the API's
    # infra: the revision name is not ignored, or gcloud's is re-sent → 409).
    # `scaling` is ignored too, as there: a later `gcloud run` never fights
    # tofu over it, so max_instances in variables.tf is the value at
    # creation; changing it afterwards is a `tofu apply -replace` or a
    # gcloud update, written down in infra/README.md.
    ignore_changes = [template[0].containers[0].image, scaling, client, client_version]
  }
  depends_on = [google_service_account_iam_member.deployer_acts_as]
}

# Public: a learning page with no account and nothing to protect. Cloudflare
# is in front (WAF, rate limits); dev is meant to sit behind Access (README).
resource "google_cloud_run_v2_service_iam_member" "public" {
  for_each = var.envs
  name     = google_cloud_run_v2_service.app[each.key].name
  location = var.region
  role     = "roles/run.invoker"
  member   = "allUsers"
}
