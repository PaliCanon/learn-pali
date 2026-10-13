// The Worker's contract (task 031): it relays to ORIGIN and adds
// `x-origin-auth` from its secret `ORIGIN_AUTH`; without that secret it
// answers 500 and calls nothing (a dropped secret is loud, not a storm of
// 403s from the service); the rest of the relay (host handling, body,
// manual redirects, the `Location` rewrite) is unchanged.
// Run: node --test infra/worker/proxy.test.mjs
import assert from "node:assert/strict";
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
