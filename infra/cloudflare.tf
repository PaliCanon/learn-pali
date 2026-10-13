# The public name of the site: <host> → a Worker that relays to Cloud Run.
# Cloudflare sits in front (WAF, bot fight, per-IP rate limits). The zone
# itself is the site's (palisearch infra).

data "cloudflare_zone" "site" {
  name = "palicanon.app"
}

# Access on the hosts that set `access` (dev; prod public later), as the
# site's dev (palisearch infra): an application on the exact host and one
# allow policy on var.admin_emails. Nothing but the listed identities passes;
# there is no service token (its secret would sit in the tofu state).
locals {
  access_envs = { for k, v in var.envs : k => v if v.access }
}

resource "cloudflare_zero_trust_access_application" "app" {
  for_each         = local.access_envs
  zone_id          = data.cloudflare_zone.site.id
  name             = "learn-pali ${each.key}"
  domain           = each.value.host
  type             = "self_hosted"
  session_duration = "24h"
}

resource "cloudflare_zero_trust_access_policy" "app_allow" {
  for_each       = local.access_envs
  application_id = cloudflare_zero_trust_access_application.app[each.key].id
  zone_id        = data.cloudflare_zone.site.id
  name           = "publisher only"
  precedence     = 1
  decision       = "allow"
  include {
    email = var.admin_emails
  }
}

resource "cloudflare_workers_script" "proxy" {
  for_each   = var.envs
  account_id = var.cloudflare_account_id
  name       = "learn-pali-${each.key}"
  content    = file("${path.module}/worker/proxy.js")
  module     = true
  # Pins the Workers runtime behaviour the script was written against.
  compatibility_date = "2026-10-08"

  plain_text_binding {
    name = "ORIGIN"
    text = google_cloud_run_v2_service.app[each.key].uri
  }
}

# The Worker is served on each host by a Workers Custom Domain. Cloudflare
# creates the proxied DNS record itself and issues the certificate for the
# exact host, at any depth (learn.dev.palicanon.app is outside Universal
# SSL's `*.palicanon.app`), so there is no record and no route to declare.
resource "cloudflare_workers_domain" "app" {
  for_each    = var.envs
  account_id  = var.cloudflare_account_id
  zone_id     = data.cloudflare_zone.site.id
  hostname    = each.value.host
  service     = cloudflare_workers_script.proxy[each.key].name
  environment = "production"

  depends_on = [
    cloudflare_zero_trust_access_application.app,
    cloudflare_zero_trust_access_policy.app_allow,
  ]
}
