// learn.palicanon.app / learn-dev.palicanon.app → learn-pali on Cloud Run.
//
// Cloudflare is the front door (WAF, bot fight, rate limits per IP); this
// relays the request as is — method, headers, body — to ORIGIN, the Cloud
// Run URL. Nothing is cached here: the pages are server-rendered per
// request, and the hashed files under /pkg/ are cheap enough from Cloud Run
// for a service that scales to zero (an edge cache for them is a possible
// later task, not part of this one).
//
// Redirects are relayed untouched (`redirect: "manual"`), except one thing:
// a `Location` pointing at the *.run.app origin is rewritten to the public
// host, or a redirect would send the visitor round the front door.
export default {
  async fetch(request, env) {
    const incoming = new URL(request.url);
    const origin = new URL(env.ORIGIN).origin;
    const target = new URL(incoming.pathname + incoming.search, origin);
    const headers = new Headers(request.headers);
    headers.delete("host");
    headers.set("x-forwarded-host", incoming.host);
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
