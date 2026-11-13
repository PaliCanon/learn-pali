terraform {
  required_version = ">= 1.8"

  # Its own state, next to the site's (prefix "palicanon") and the API's
  # (prefix "pali-canon-api") in the same bucket.
  backend "gcs" {
    bucket = "palicanon-app-tofu-state"
    prefix = "learn-pali"
  }

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 6.0"
    }
  }
}

# Credentials come from the environment: gcloud auth application-default
# login. Nothing is read from a file in this repository.
provider "google" {
  project = var.gcp_project
  region  = var.region
}
