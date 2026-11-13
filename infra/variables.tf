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

# One entry per environment: prod only (task 082: the dev environment is
# gone). `envs` is replaced as a whole by an override, so prod lives in this
# default, not in a git-ignored tfvars (036).
#
# `access` has no resource left (cloudflare.tf: the Access resources and the
# `admin_emails` variable went with dev). It stays false, for deploy.yml's
# ACCESS_prod mirror.
variable "envs" {
  type = map(object({
    host          = string
    max_instances = number
    access        = optional(bool, false)
  }))
  default = {
    prod = {
      host          = "learn.palicanon.app"
      max_instances = 3
      access        = false
    }
  }
}
