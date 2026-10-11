# The public name of the site: <host> → a Worker that relays to Cloud Run.
# Cloudflare sits in front (WAF, bot fight, per-IP rate limits). The zone
# itself is the site's (palisearch infra).

data "cloudflare_zone" "site" {
  name = "palicanon.app"
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
}
