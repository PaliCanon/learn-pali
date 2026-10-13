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
#
# `access` puts the host behind Cloudflare Access (cloudflare.tf): true for
# dev (user, 2026-10-12: dev behind Cloudflare with auth), false when omitted,
# so prod stays public until it is decided otherwise.
variable "envs" {
  type = map(object({
    host          = string
    max_instances = number
    access        = optional(bool, false)
  }))
  default = {
    dev = {
      host          = "learn.dev.palicanon.app"
      max_instances = 2
      access        = true
    }
    # prod = {
    #   host          = "learn.palicanon.app"
    #   max_instances = 3
    # }
  }
}

# Who passes Cloudflare Access on the hosts that set `access`. An email
# address is not a secret, but it is not written here either: pass it as
# `TF_VAR_admin_emails='["you@example.com"]'` or in a git-ignored tfvars
# file (README, "Only the user, in order"; prod.tfvars.example shows the
# shape). No default on purpose: an apply without it fails instead of
# opening the dev host to nobody or to anyone.
variable "admin_emails" {
  description = "Email addresses allowed through Cloudflare Access on the hosts with access = true."
  type        = list(string)

  validation {
    condition     = length(var.admin_emails) > 0 && alltrue([for e in var.admin_emails : can(regex("^[^@\\s]+@[^@\\s]+$", e))])
    error_message = "admin_emails must hold at least one address (name@domain)."
  }
}
