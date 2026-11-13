# Nothing of Cloud Run is left here (task 083): prod runs on the user's server
# (ssh.lsh.tech, the lsh-tech stack), which reads the API token below from
# Secret Manager.

# ---------------------------------------------------------------- Drive API

# The progress sync (task 050) calls the Drive API from the learner's
# browser, with the learner's own token (scope drive.appdata): the project
# must have the API on. It was enabled by hand on 2026-10-16; the import
# adopts it, so the first plan shows an import and no change. Never
# disabled on destroy: the PaliCanon app's sync uses it too. The OAuth
# clients and the consent screen are console-only (no tofu resource).
import {
  to = google_project_service.drive
  id = "${var.gcp_project}/drive.googleapis.com"
}

resource "google_project_service" "drive" {
  project            = var.gcp_project
  service            = "drive.googleapis.com"
  disable_on_destroy = false
}

# ---------------------------------------------------------------- secrets

# The API token (062): the bearer the site's server sends to the PaliCanon
# API for the dictionary search and a word's passage. Tofu creates the
# secret and NEVER a version (the value would sit in its state):
# `infra/scripts/api-token create <env>` adds the version from the token
# issued by the API's `scripts/token new learn --env <env>`. The lsh-tech
# stack reads it; no identity of this repository does.
resource "google_secret_manager_secret" "api_token" {
  for_each  = var.envs
  secret_id = "learn-pali-api-token-${each.key}"
  replication {
    auto {}
  }
}
