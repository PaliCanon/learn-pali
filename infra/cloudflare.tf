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

# A Worker route needs a proxied record to attach to; 100:: is the
# documented placeholder (traffic never reaches it).
resource "cloudflare_record" "app" {
  for_each = var.envs
  zone_id  = data.cloudflare_zone.site.id
  name     = each.value.host
  type     = "AAAA"
  content  = "100::"
  proxied  = true
  comment  = "learn-pali (${each.key}): served by the Worker"
}

resource "cloudflare_workers_route" "app" {
  for_each    = var.envs
  zone_id     = data.cloudflare_zone.site.id
  pattern     = "${each.value.host}/*"
  script_name = cloudflare_workers_script.proxy[each.key].name
}
