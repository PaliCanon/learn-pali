variable "gcp_project" {
  description = "The GCP project, shared with the site and the API."
  type        = string
  default     = "palicanon-app"
}

variable "region" {
  type    = string
  default = "europe-west1"
}

variable "bootstrap_image" {
  description = "Placeholder image for the first apply, before CI has pushed anything."
  type        = string
  default     = "us-docker.pkg.dev/cloudrun/container/hello"
}

variable "cloudflare_account_id" {
  type    = string
  default = "c0690c72f1b7c85737052c3a9ebac154"
}

# One entry per environment. The dev host is learn.dev.palicanon.app (the
# user's choice, 2026-10-11), two levels under the zone: Universal SSL covers
# `*.palicanon.app` only, so every host is served by a Workers Custom Domain
# (cloudflare.tf), for which Cloudflare issues the certificate of the exact
# name, whatever its depth. Changing a host is an edit here and a tofu apply.
#
# The default holds `dev` ONLY: the first apply must not stand up a public
# prod host before dev has deployed and been checked from CI. Prod is added
# afterwards, with `-var-file` or by uncommenting the block below (README,
# "Order"), then a second apply.
variable "envs" {
  type = map(object({
    host          = string
    max_instances = number
  }))
  default = {
    dev = {
      host          = "learn.dev.palicanon.app"
      max_instances = 2
    }
    # prod = {
    #   host          = "learn.palicanon.app"
    #   max_instances = 3
    # }
  }
}
