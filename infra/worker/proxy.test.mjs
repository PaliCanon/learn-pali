// The Worker's contract (task 031): it relays to ORIGIN and adds
// `x-origin-auth` from its secret `ORIGIN_AUTH`; without that secret it
// answers 500 and calls nothing (a dropped secret is loud, not a storm of
// 403s from the service); the rest of the relay (host handling, body,
// manual redirects, the `Location` rewrite) is unchanged.
// Run: node --test infra/worker/proxy.test.mjs
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { afterEach, test } from "node:test";

import worker from "./proxy.js";

const ORIGIN = "https://learn-pali-dev-abc123-ew.a.run.app";
const HOST = "https://learn.dev.palicanon.app";
const SECRET = "origin-secret-for-tests-0123456789abcdef";
const realFetch = globalThis.fetch;

/** Replace `fetch` with a recorder answering `respond(url, init)`. */
function stubFetch(respond = () => new Response("ok")) {
  const calls = [];
  globalThis.fetch = async (url, init) => {
    calls.push({ url: String(url), init });
    return respond(String(url), init);
  };
  return calls;
}

afterEach(() => {
  globalThis.fetch = realFetch;
});

const env = (extra = {}) => ({ ORIGIN, ORIGIN_AUTH: SECRET, ...extra });

test("adds x-origin-auth from the secret on every relayed request", async () => {
  const calls = stubFetch();
  await worker.fetch(new Request(`${HOST}/pkg/app.js?v=1`), env());
  assert.equal(calls.length, 1);
  assert.equal(calls[0].init.headers.get("x-origin-auth"), SECRET);
  assert.equal(calls[0].url, `${ORIGIN}/pkg/app.js?v=1`);
});

test("overwrites an x-origin-auth sent by the client", async () => {
  const calls = stubFetch();
  const request = new Request(`${HOST}/`, { headers: { "x-origin-auth": "guess" } });
  await worker.fetch(request, env());
  assert.equal(calls[0].init.headers.get("x-origin-auth"), SECRET);
});

test("an x-origin-auth sent by the client never reaches the origin, in any case or number", async () => {
  const calls = stubFetch();
  const sent = new Headers({ "x-origin-auth": "guess" });
  sent.append("X-Origin-Auth", "another guess");
  await worker.fetch(new Request(`${HOST}/`, { headers: sent }), env());
  const upstream = [...calls[0].init.headers].filter(([name]) => name.toLowerCase() === "x-origin-auth");
  assert.deepEqual(upstream, [["x-origin-auth", SECRET]]);
});

test("answers 500 and calls nothing when the secret is absent", async () => {
  const calls = stubFetch();
  const response = await worker.fetch(new Request(`${HOST}/`), { ORIGIN });
  assert.equal(response.status, 500);
  assert.equal(calls.length, 0);
});

test("answers 500 and calls nothing when the secret is blank", async () => {
  for (const blank of ["", "   ", "\n"]) {
    const calls = stubFetch();
    const response = await worker.fetch(new Request(`${HOST}/`), env({ ORIGIN_AUTH: blank }));
    assert.equal(response.status, 500, JSON.stringify(blank));
    assert.equal(calls.length, 0, JSON.stringify(blank));
  }
});

test("the 500 holds no secret, no origin and is not cached", async () => {
  stubFetch();
  const response = await worker.fetch(new Request(`${HOST}/`), env({ ORIGIN_AUTH: "" }));
  const body = await response.text();
  assert.ok(!body.includes(ORIGIN));
  assert.ok(!body.includes(SECRET));
  assert.equal(response.headers.get("cache-control"), "no-store");
});

test("a secret with a trailing newline is sent trimmed", async () => {
  const calls = stubFetch();
  await worker.fetch(new Request(`${HOST}/`), env({ ORIGIN_AUTH: `${SECRET}\n` }));
  assert.equal(calls[0].init.headers.get("x-origin-auth"), SECRET);
});

test("deletes host and sets x-forwarded-host", async () => {
  const calls = stubFetch();
  const request = new Request(`${HOST}/`, { headers: { host: "evil.example" } });
  await worker.fetch(request, env());
  const headers = calls[0].init.headers;
  assert.equal(headers.get("host"), null);
  assert.equal(headers.get("x-forwarded-host"), "learn.dev.palicanon.app");
});

test("relays the body of a POST, and none for a GET", async () => {
  const calls = stubFetch();
  const post = new Request(`${HOST}/api/some_server_fn`, {
    method: "POST",
    body: "a=1",
    headers: { "content-type": "application/x-www-form-urlencoded" },
  });
  await worker.fetch(post, env());
  assert.equal(calls[0].init.method, "POST");
  assert.equal(await new Response(calls[0].init.body).text(), "a=1");
  await worker.fetch(new Request(`${HOST}/`), env());
  assert.equal(calls[1].init.body, undefined);
});

test("does not follow redirects (the browser does, on the public host)", async () => {
  const calls = stubFetch();
  await worker.fetch(new Request(`${HOST}/`), env());
  assert.equal(calls[0].init.redirect, "manual");
});

test("rewrites a Location to the run.app origin onto the incoming host (unchanged)", async () => {
  stubFetch(() => new Response(null, { status: 302, headers: { location: `${ORIGIN}/deck?x=1` } }));
  const response = await worker.fetch(new Request(`${HOST}/old`), env());
  assert.equal(response.status, 302);
  assert.equal(response.headers.get("location"), `${HOST}/deck?x=1`);
});

test("leaves a Location elsewhere untouched (unchanged)", async () => {
  stubFetch(() => new Response(null, { status: 302, headers: { location: "https://example.org/x" } }));
  const response = await worker.fetch(new Request(`${HOST}/old`), env());
  assert.equal(response.headers.get("location"), "https://example.org/x");
});

// Task 051 (urgent, user 2026-10-16; the finding is anatta-sso 066's review
// delta 1, confirmed by a run in anatta, message 10754): `new URL(pathname +
// search, origin)` RESOLVES the path, and a path starting `//` (or `/\`,
// which the URL parser reads as `//`) is a scheme-relative reference — the
// Worker fetched the attacker's host carrying x-origin-auth and every client
// header. The Worker now sets the path on ORIGIN and refuses such a path
// outright. These four tests are anatta-console 068's and anatta-sso 066's,
// on this service's host.

/** The request paths that try to leave ORIGIN's host. */
const ESCAPES = [
  "//attacker.example/x",
  "/\\attacker.example/x",
  "/%5Cattacker.example/x",
  "//",
];

/** A request-like object whose `url` is the raw string, as no `Request` would normalise it. */
const raw = (url) => ({ url, method: "GET", headers: new Headers() });

test("no request path sends the fetch, or the secret, to another host", async () => {
  const host = new URL(ORIGIN).host;
  for (const path of ESCAPES) {
    for (const request of [new Request(`https://learn.dev.palicanon.app${path}`), raw(`https://learn.dev.palicanon.app${path}`)]) {
      const calls = stubFetch();
      const response = await worker.fetch(request, env());
      for (const call of calls) {
        const elsewhere = new URL(call.url).host !== host;
        assert.ok(
          !(elsewhere && call.init.headers.get("x-origin-auth") !== null),
          `${JSON.stringify(path)} sent x-origin-auth to ${call.url}`,
        );
        assert.equal(new URL(call.url).host, host, `${JSON.stringify(path)} fetched ${call.url}`);
      }
      assert.ok(calls.length === 1 || response.status === 400, `${JSON.stringify(path)}: ${response.status}`);
    }
  }
});

test("a path that would leave ORIGIN's host is a 400, never fetched", async () => {
  for (const path of ["//attacker.example/x", "/\\attacker.example/x", "//"]) {
    for (const request of [new Request(`https://learn.dev.palicanon.app${path}`), raw(`https://learn.dev.palicanon.app${path}`)]) {
      const calls = stubFetch();
      const response = await worker.fetch(request, env());
      assert.equal(response.status, 400, JSON.stringify(path));
      assert.equal(calls.length, 0, JSON.stringify(path));
      assert.equal(response.headers.get("cache-control"), "no-store");
      assert.ok(!(await response.text()).includes(SECRET));
    }
  }
});

test("an encoded backslash stays a path on ORIGIN", async () => {
  const calls = stubFetch();
  await worker.fetch(new Request("https://learn.dev.palicanon.app/%5Cattacker.example/x?q=1"), env());
  assert.equal(calls.length, 1);
  assert.equal(calls[0].url, `${ORIGIN}/%5Cattacker.example/x?q=1`);
  assert.equal(calls[0].init.headers.get("x-origin-auth"), SECRET);
});

test("the target is ORIGIN with the path set, never the path resolved against it", async () => {
  // The 400 above refuses every path a resolution would take elsewhere, so a
  // target resolved again behind it behaves the same: this reads the source.
  const source = await readFile(new URL("./proxy.js", import.meta.url), "utf8");
  assert.ok(source.includes("target.pathname = incoming.pathname;"), "proxy.js does not set the target's path");
  assert.ok(source.includes("target.search = incoming.search;"), "proxy.js does not set the target's query");
  assert.ok(
    !/new URL\([^)]*pathname[^)]*,\s*(origin|env\.ORIGIN)\)/.test(source),
    "proxy.js resolves a path against the origin",
  );
});

test("a body dropped on GET and HEAD drops its length headers too", async () => {
  // A content-length or transfer-encoding left on a request with no body
  // makes the origin wait for bytes that never come (anatta review 066, #10900).
  for (const method of ["GET", "HEAD"]) {
    const calls = stubFetch();
    const request = new Request("https://learn.dev.palicanon.app/", {
      method,
      headers: { "content-length": "5", "transfer-encoding": "chunked", "x-keep": "yes" },
    });
    assert.equal(request.headers.get("content-length"), "5", "the request must carry the header for the test to mean anything");
    await worker.fetch(request, env());
    const sent = calls[0].init.headers;
    assert.equal(sent.get("content-length"), null, method);
    assert.equal(sent.get("transfer-encoding"), null, method);
    assert.equal(sent.get("x-keep"), "yes", method);
    assert.equal(calls[0].init.body, undefined, method);
  }
});

test("a POST keeps its body and its content-length", async () => {
  const calls = stubFetch();
  const request = new Request("https://learn.dev.palicanon.app/", {
    method: "POST",
    headers: { "content-length": "3", "content-type": "application/x-www-form-urlencoded" },
    body: "x=1",
  });
  await worker.fetch(request, env());
  assert.equal(calls[0].init.headers.get("content-length"), "3");
  assert.equal(await new Response(calls[0].init.body).text(), "x=1");
});

// Review round 1 (T1, T2, T3): a path that only LOOKS like an escape, or a
// `//` anywhere but the start of the path, is a legitimate request — relayed
// to ORIGIN as is, never refused.

test("an encoded backslash is fetched on ORIGIN, as a Request and as a raw url", async () => {
  const host = new URL(ORIGIN).host;
  const url = "https://learn.dev.palicanon.app/%5Cattacker.example/x?q=1";
  for (const request of [new Request(url), raw(url)]) {
    const calls = stubFetch();
    const response = await worker.fetch(request, env());
    assert.notEqual(response.status, 400);
    assert.equal(calls.length, 1);
    assert.equal(new URL(calls[0].url).host, host);
    assert.equal(calls[0].url, `${ORIGIN}/%5Cattacker.example/x?q=1`);
    assert.equal(calls[0].init.headers.get("x-origin-auth"), SECRET);
  }
});

test("a // later in the path or in the query is relayed unchanged to ORIGIN", async () => {
  for (const path of ["/a//b", "/?next=//x", "/a//b?next=//x&to=https://example.org//y"]) {
    for (const request of [new Request(`https://learn.dev.palicanon.app${path}`), raw(`https://learn.dev.palicanon.app${path}`)]) {
      const calls = stubFetch();
      const response = await worker.fetch(request, env());
      assert.notEqual(response.status, 400, path);
      assert.equal(calls.length, 1, path);
      assert.equal(calls[0].url, `${ORIGIN}${path}`, path);
    }
  }
});
