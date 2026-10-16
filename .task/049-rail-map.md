# 049 the rail lists Map

status: open · rank: 1 · level: simple · type: fix · request: user, 2026-10-16, in the vizualisation session: «il faut rapidement corriger les redirections. que chacun des sites ai les onglets» · light path

## Summary
Rank: difficulty 1 (one entry, one key in three catalogues, its tests), impact 2 (a service missing from the rail on every page) → 1.
Every site's rail lists the three services in the same order with the same hrefs: Search (https://search.palicanon.app), Learn (this site, `/`), Map (https://map.palicanon.app), as vizualisation's rail already does. Learn's rail lacked Map. Fix: a Map entry (`IconName::Motion` stands in, as in vizualisation), `nav_map` in en/fr/vi (Map, Carte, Bản đồ), the rail's tests extended to the third entry. The shared services list (one source for every rail) is a separate request, designed later. Deployed from the laptop while GitHub Actions is blocked.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | red (a mutation: Map's href changed): thread 'app::tests::the_rail_lists_search_then_learn_then_map' panicked at app/src/app.rs:232:14; green: fmt 0, clippy ssr --all-targets 0, hydrate wasm32 0, csr wasm32 --all-targets 0, test ssr 0 (390 + 9 passed) |
