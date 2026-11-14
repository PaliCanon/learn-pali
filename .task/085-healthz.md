# 085 GET /healthz

status: in review
rank: 1
level: simple
request: the coordinator, 2026-11-14: one uniform `GET /healthz` on every public service, for external uptime checks

## Summary

`GET /healthz` answers 200 with the body `ok` and `Cache-Control: no-store`, from `origin::router` itself: no page render, no server function. It sits behind the origin guard like every route (open in prod on the server, where no secret is set).

## Log

| Step | Result |
| --- | --- |
| Red | `cargo test --locked -p learn_pali --features ssr --test healthz`: `healthz_answers_200_ok_uncached_without_the_app` FAILED (the route fell to the app's 404 page), exit 101 |
| Green | same command: 2 passed, exit 0 |
| Checks | `cargo fmt --all --check`, `leptosfmt --check app`, `cargo clippy --locked -p learn_pali --features ssr --all-targets -- -D warnings`: exit 0 |
