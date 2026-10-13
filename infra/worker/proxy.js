// learn.palicanon.app / learn.dev.palicanon.app → learn-pali on Cloud Run.
//
// Cloudflare is the front door (WAF, bot fight, rate limits per IP); this
// relays the request as is — method, headers, body — to ORIGIN, the Cloud
// Run URL, and adds the one header the service demands: `x-origin-auth`, the
// secret `ORIGIN_AUTH` of this Worker (the service answers 403 to any request
// without it, so the *.run.app URL is no way round Cloudflare). The secret is
// set through the Cloudflare API (`infra/scripts/origin-secret push`), never
// by tofu; if it is missing (an apply that re-uploaded the script can drop
// it) the Worker answers 500 and calls nothing, so a dropped secret shows at
// once instead of as a storm of 403s from the service.
//
// Nothing is cached here: the pages are server-rendered per
// request, and the hashed files under /pkg/ are cheap enough from Cloud Run
// for a service that scales to zero (an edge cache for them is a possible
// later task, not part of this one).
//
// Redirects are relayed untouched (`redirect: "manual"`), except one thing:
// a `Location` pointing at the *.run.app origin is rewritten to the public
// host, or a redirect would send the visitor round the front door.
/** A 500 that is never cached and says what is missing, not its value. */
function misconfigured(what) {
  return new Response(`The ${what} is not configured.`, {
    status: 500,
    headers: { "content-type": "text/plain", "cache-control": "no-store" },
  });
}

export default {
  async fetch(request, env) {
    const secret = typeof env.ORIGIN_AUTH === "string" ? env.ORIGIN_AUTH.trim() : "";
    if (secret === "") return misconfigured("origin secret");
    const incoming = new URL(request.url);
    const origin = new URL(env.ORIGIN).origin;
    const target = new URL(incoming.pathname + incoming.search, origin);
    const headers = new Headers(request.headers);
    headers.delete("host");
    headers.set("x-forwarded-host", incoming.host);
    // `set` replaces any value the client sent, whatever its case or number.
    headers.set("x-origin-auth", secret);
    const response = await fetch(target, {
      method: request.method,
      headers,
      body: ["GET", "HEAD"].includes(request.method) ? undefined : request.body,
      redirect: "manual",
    });
    const location = response.headers.get("location");
    if (location && location.startsWith(origin)) {
      const relayed = new Response(response.body, response);
      relayed.headers.set("location", incoming.origin + location.slice(origin.length));
      return relayed;
    }
    return response;
  },
};
