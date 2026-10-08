output "urls" {
  description = "The Cloud Run URL of each environment (the Worker's ORIGIN)."
  value       = { for k, s in google_cloud_run_v2_service.app : k => s.uri }
}

output "hosts" {
  description = "The public address of each environment."
  value       = { for k, v in var.envs : k => "https://${v.host}" }
}

output "runtime_service_accounts" {
  description = "The identity each service runs as (no roles)."
  value       = { for k, a in google_service_account.app : k => a.email }
}
