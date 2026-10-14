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
# `access` puts the host behind Cloudflare Access (cloudflare.tf). It is false
# for dev and prod alike (user, 2026-10-13): dev is public behind the origin
# guard, as search-front's dev is (search-dev.palicanon.app). The Access
# resources stay behind the flag for a host that needs them later, and then
# `admin_emails` is required (below).
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
      access        = false
    }
    # Prod lives in this default, not in a git-ignored tfvars: `envs` is
    # replaced as a whole, so an apply without that file would destroy prod (036).
    prod = {
      host          = "learn.palicanon.app"
      max_instances = 3
      access        = false
    }
  }
}

# Who passes Cloudflare Access on the hosts that set `access = true`. Empty
# by default: no env has Access, so nothing is needed (an apply asks for no
# address). An email address is not a secret, but it is not written here
# either: pass it as `TF_VAR_admin_emails='["you@example.com"]'` or in a
# git-ignored tfvars file (README; prod.tfvars.example shows the shape).
# An env with `access = true` and no address is refused (the second
# validation: it reads `var.envs`, OpenTofu 1.9+) instead of opening the host
# to nobody; a malformed address is refused whether an env needs it or not.
variable "admin_emails" {
  description = "Email addresses allowed through Cloudflare Access on the hosts with access = true."
  type        = list(string)
  default     = []

  validation {
    condition     = alltrue([for e in var.admin_emails : can(regex("^[^@\\s]+@[^@\\s]+$", e))])
    error_message = "Every admin_emails entry must be an address (name@domain)."
  }

  validation {
    condition     = !anytrue([for env in values(var.envs) : env.access]) || length(var.admin_emails) > 0
    error_message = "An env sets access = true, so admin_emails must hold at least one address (name@domain)."
  }
}
