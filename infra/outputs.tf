output "hosts" {
  description = "The public address of each environment."
  value       = { for k, v in var.envs : k => "https://${v.host}" }
}

output "api_token_secrets" {
  description = "The Secret Manager secret of each environment (no version in tofu: scripts/api-token create)."
  value       = { for k, s in google_secret_manager_secret.api_token : k => s.secret_id }
}
