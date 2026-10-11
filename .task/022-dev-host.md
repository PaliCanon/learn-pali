# 022 The dev host is learn.dev.palicanon.app

status: building · rank: 2 · level: simple · type: build · path: light · request: learn-pali (user, 2026-10-11: « l'url doit être learn.dev.palicanon.app »)

## Summary
Dev is served at `learn.dev.palicanon.app` (was the proposal `learn-dev.palicanon.app`); prod stays `learn.palicanon.app`. The user's choice departs from the convention (docs/conventions/deploy.md: "dev hosts carry -dev … one level of subdomain stays under Cloudflare's free certificate, deeper names are Workers Custom Domains or Pages domains") — so the dev host cannot use the route + `AAAA 100::` record pattern: Universal SSL covers `*.palicanon.app` only, not `*.dev.palicanon.app`. Decided: every host is served by a **Workers Custom Domain** (`cloudflare_workers_domain`: Cloudflare creates the DNS record and issues the certificate for the exact name, any depth), replacing the record + route pair for both envs (one mechanism, prod included). The convention gets the line that a service may use `<svc>.dev.palicanon.app` through a Workers Custom Domain (decision written in docs). Rank 2: infra text, validated with `tofu validate`, nothing applied.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Define | done | the user's words; the certificate depth checked against the convention |

## Retro
