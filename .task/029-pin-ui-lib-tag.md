# 029 learn-pali builds against ui-lib v0.1.0, by tag

status: merged · rank: 2 · level: simple · type: build · request: user, 2026-10-12 ("tag ui-lib et pin learn-pali dessus") · light path · after: ui-lab 048 (the release design)

## Summary
Rank: difficulty 2 (one manifest line, the lockfile, the ui-lib checkout of every CI and deploy job), impact 2 (no behaviour; the build becomes reproducible) → 2.
ui-lib's first tag is `v0.1.0` (annotated, on ui-lib main 90378bb, pushed 2026-10-12 by the orchestrator). Task 048 had settled `v0.0.1`, but ui-lib's manifest already said 0.1.0 and learn-pali requires `version = "0.1"`; tagging the manifest's own version keeps the guard `version == tag` true without a downgrade. The tag is the baseline and is never moved.
Decided (048 consensus v3): the consumer pins `tag = "v0.1.0"` (keeping `version = "0.1"`); the local `[patch]` to `../ui-lib` stays for development; on a runner the `../ui-lib` checkout is made at the tag read from `app/Cargo.toml`, then guarded: `ui-lib/Cargo.toml` version must equal the tag without its `v`. One local composite action does checkout, guard and ssh agent, so the seven CI jobs and the deploy job do not repeat it.
Ruled out: release-plz on ui-lib (048's own lane); search-front and the ui-lab gallery (their own tasks).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Tag | done | ui-lib v0.1.0 → 90378bb, pushed |
| Build | done | ee3d89a, 5770aca; grep 0, 7, 1; YAML ok; fmt, clippy, test ✓ |
| Check | done | orchestrator, tip da8615a, ../ui-lib 90378bb (= v0.1.0): diff read; Cargo fetched the tag (git checkout 90378bb), `cargo metadata --locked` 0, the lockfile needs no change (the patched path dependency carries no source); non-app crates 112 tests ok. No visible result: checked by the orchestrator. Found on main, not caused here: deny.toml no longer parses (028 inserted a line inside `[[licenses.exceptions]]`) → its own task 030; rank stays 2 (the bug is in a file this task does not touch) |
| Merge | done | f4cfc2f on main (not pushed: a push deploys dev) |

## Retro
One line (light path): a pin that only edits the manifest pins nothing while a `[patch]` path is in force; the runner's checkout is what must move, so the tag is read from the manifest in one place.
