# Shared with the site's infra (palisearch): read, never managed here.
data "google_service_account" "deployer" {
  account_id = "github-deployer"
}

# The Secret Manager API is enabled by the API's infra (`google_project_service`
# there); a project-wide switch is not declared twice.

# ---------------------------------------------------------------- secrets

# The origin secret: the value the Worker sends as `x-origin-auth` and the
# service requires (403 without it). Tofu creates the secret and NEVER a
# version: the value would sit in its state. `infra/scripts/origin-secret
# create <env>` adds the version (a random value that never leaves Secret
# Manager and the Worker's secret store).
resource "google_secret_manager_secret" "origin" {
  for_each  = var.envs
  secret_id = "learn-pali-origin-secret-${each.key}"
  replication {
    auto {}
  }
}

# ---------------------------------------------------------------- service account

# The identity the service runs as. Its one role is the right to read the
# origin secret, on that secret only: the site reads nothing else from GCP
# (the deck is embedded in the binary, the progress lives in the visitor's
# browser). A role added here is a decision to write down, not a default.
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

resource "google_secret_manager_secret_iam_member" "app_reads_origin" {
  for_each  = var.envs
  secret_id = google_secret_manager_secret.origin[each.key].id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.app[each.key].email}"
}

# The rollout checks the new revision BEFORE it takes traffic, through its
# tagged URL, with the origin header (`scripts/rollout-check.sh`): CI reads
# the origin secret and nothing else. Consequence (task 031, as search-front
# 007's R3): every repository listed in `github_repos` of the site's infra can
# read this one secret.
resource "google_secret_manager_secret_iam_member" "deployer_reads_origin" {
  for_each  = var.envs
  secret_id = google_secret_manager_secret.origin[each.key].id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${data.google_service_account.deployer.email}"
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
      # from the files they describe. RUST_LOG is the only value here.
      env {
        name  = "RUST_LOG"
        value = "info"
      }
      # By reference to the secret, never a value (it would sit in the state
      # and in the revision JSON). Read when an instance starts: a new
      # version needs a new revision (README, "Rotation").
      env {
        name = "ORIGIN_SECRET"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.origin[each.key].secret_id
            version = "latest"
          }
        }
      }
      # TCP, not HTTP (as search-front 007's R2): the service answers 403 to
      # a request without the origin header, and a Cloud Run HTTP probe header
      # is a plain string in the service resource, which would put the origin
      # secret in tofu state. The page itself is checked by the rollout, on
      # the tagged revision, before traffic moves.
      startup_probe {
        tcp_socket {
          port = 8080
        }
        # The margin covers a slow image pull on a cold start.
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
  depends_on = [
    google_secret_manager_secret_iam_member.app_reads_origin,
    google_service_account_iam_member.deployer_acts_as,
  ]
}

# Invokable by anyone at the Cloud Run layer: the Worker has no Google
# identity to present. What keeps the `*.run.app` URL from being a way round
# Cloudflare (its WAF, its limits, dev's Access) is the service itself: it
# refuses (403) any request without the origin secret (deploy.md).
# One exception, accepted: before the first deploy (and after a rollback of
# the first deploy, which puts the traffic back on it) `*.run.app` serves
# the bootstrap hello container (var.bootstrap_image). It has no guard, but
# it is Google's sample page: no app, no data, nothing to protect.
resource "google_cloud_run_v2_service_iam_member" "public" {
  for_each = var.envs
  name     = google_cloud_run_v2_service.app[each.key].name
  location = var.region
  role     = "roles/run.invoker"
  member   = "allUsers"
}
