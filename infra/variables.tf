variable "gcp_project" {
  description = "The GCP project, shared with the site and the API."
  type        = string
  default     = "palicanon-app"
}

variable "region" {
  type    = string
  default = "europe-west1"
}

# One entry per environment: prod only (task 082: the dev environment is
# gone). `envs` is replaced as a whole by an override, so prod lives in this
# default, not in a git-ignored tfvars (036).
variable "envs" {
  type = map(object({
    host = string
  }))
  default = {
    prod = {
      host = "learn.palicanon.app"
    }
  }
}
