# 040 hydrate from ui-lib's root past Cloudflare's injected nodes

status: building · rank: 8 · level: hard · type: fix · request: ui-lib 063 (user, 2026-10-15); closes 039 · session: none (the ui-lib session holds the design)

## Summary
Rank: difficulty 5 (the site's hydration entry, its shell, a headless-Chrome check in the deploy's public-host step), impact 8 (learn.dev and learn.palicanon.app are dead in a browser until it ships) → 8, the rank of ui-lib 063 this task is the site side of.
Cause (ui-lib 063, learn-pali 039): Cloudflare injects a hidden `<a href="…/cdn-cgi/content?id=…">` (an AI-crawler decoy) as the FIRST child of `<body>`, plus the Web Analytics beacon and challenge scripts after the app. `leptos::mount::hydrate_body(app::App)` walks `<body>`'s children in render order, meets the decoy where it expects the app's first node, and panics (`tachys-0.2.19/src/hydration.rs:227`, `unreachable!()` in release): the page stays on "Loading your progress…".
Design (ui-lib 063 v5, validated by the user 2026-10-15, nodes SITES, T3, ROLL): ui-lib v0.1.2 ships `HydrationRoot` (`<div id="pc-app">`, `APP_ROOT_ID`) and `palicanon_ui_lib::hydrate(f)` (hydrates from `#pc-app`, then sets `<html data-hydrated>`; a missing root panics, no fallback). This site:
- pins ui-lib `v0.1.2` (`app/Cargo.toml`, `Cargo.lock`);
- `shell()` renders `<body><HydrationRoot><App/></HydrationRoot></body>` and `hydrate()` calls `palicanon_ui_lib::hydrate(app::App)`, in the same commit (the id written and the id read); the decoy and the scripts stay `<body>` children outside the root;
- T3: `scripts/rollout-check.sh --via-worker` (the public host; Chrome cannot add the origin header) ends with a `hydrate` check: headless Chrome (`--headless=new --enable-logging=stderr --virtual-time-budget=15000 --dump-dom`, at most 60 s) on `/` (the site's one page); `panicked at` or `RuntimeError` on Chrome's stderr, or no `data-hydrated` on `<html>` in the dumped DOM (a timeout included) → exit 1 → the deploy's retries, then the rollback; a Cloudflare challenge page → exit 3, a `::warning::` (skipped, never green), the orchestrator runs the same check by hand before any prod tag; no Chrome → 64 before any request. `deploy.yml` sets Chrome up (`browser-actions/setup-chrome@v2`) and hands its path to the step as `CHROME`.
Ruled out here (ui-lib 063): removing or moving the injected nodes; turning the zone features off (the user keeps them). Limit: a node changed INSIDE the root (Email Obfuscation) still breaks hydration; learn-pali renders no address today.
Rollout (ROLL): this PR → main → dev deploy, T3 on learn.dev; green → tag v0.1.1 → prod, T3 on learn.palicanon.app; red → no prod tag.

## Log
| Step | Result | Output |
| --- | --- | --- |
