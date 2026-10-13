output "urls" {
  description = "The Cloud Run URL of each environment (the Worker's ORIGIN)."
  value       = { for k, s in google_cloud_run_v2_service.app : k => s.uri }
}

output "hosts" {
  description = "The public address of each environment."
  value       = { for k, v in var.envs : k => "https://${v.host}" }
}

output "runtime_service_accounts" {
  description = "The identity each service runs as (one role: read its origin secret)."
  value       = { for k, a in google_service_account.app : k => a.email }
}

output "origin_secrets" {
  description = "The Secret Manager secret of each environment (no version in tofu: scripts/origin-secret create)."
  value       = { for k, s in google_secret_manager_secret.origin : k => s.secret_id }
}

output "access_hosts" {
  description = "The hosts behind Cloudflare Access."
  value       = [for k, a in cloudflare_zero_trust_access_application.app : a.domain]
}
