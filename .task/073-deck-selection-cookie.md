# 073 The selected deck survives the first paint

status: merged · rank: 3 · level: standard · type: fix · request: the user, 2026-10-24 (« des fois il m'affiche Words 0 et je suis sur le devanagari »)
worktree: /Users/lsh/projects/palicanon/wt/learn-pali-073/learn-pali (branch `fix/deck-selection-cookie`), paired `wt/learn-pali-073/ui-lib` at v0.4.5

## Summary
Rank: difficulty 3 (a device setting moved from `localStorage` to ui-lib's cookie store, a migration read, one server-side test), impact 3 (every load of the home on the phone). Light path, no agents; one review round with @deep (a value captured at one time and read at another: the design names the owner).

The selected deck was a `localStorage` key (`pc_learn_deck`), read when the browser builds its backend. The server, which paints the page first, has no `localStorage`: it painted the words deck (Words 0, its counts, its list), and the browser switched to Devanāgarī only once the WebAssembly had loaded, a second or two on a phone. That is the « Words 0 » the user sees. The theme and the language do not flash because ui-lib keeps them in a cookie the server reads from the request (`persisted.rs`). The deck selection now lives in the same place: ui-lib's `Store::current()` (`pc_learn_deck` as a cookie on the site, read by the server for the first paint and by the browser after; `localStorage` under the app's `Store::Local`, same key). A browser whose cookie is empty reads the old `localStorage` key once (today's learners keep their selection) and the next select writes the cookie. The owner of the setting is the device store; the Book never holds it (063a).

Ruled out: hiding the deck-specific content until hydration (a blank flash instead of a wrong one).

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | ✓ | 0b3cc31, c0c542b (review fixes) `DeviceStore` over ui-lib's `Store::current()` with the one-time fallback read, written through (review); `web_store()` on it; test: the document paints the cookie's deck |
| Matrix | ✓ | fmt, leptosfmt (app.rs, web.rs), clippy ssr/csr/hydrate, test ssr: 816 passed (locked on ui-lib v0.4.5) |
| Review | ✓ | round 1 @deep: 6 findings, 1 taken, 2 tests taken, 4 closed with reason or code (below) |
| Check (orchestrator) | ✓ | the branch served on 3073: `curl` with `pc_learn_deck=devanagari` paints Devanāgarī pressed and Words not; without the cookie, Words; the user confirms on the phone in prod (no flash on load) |

## Reviews
### Round 1 — 2026-10-24 (@deep, lasting session entry `review-073-deck-selection-cookie`; the only round at rank 3)
1. The fallback read never wrote the store: the flash stayed for 063a's learners — TAKEN: the fallback value is written through once.
2. The test proves the chip, not the content — closed with reason: with no progress both decks' homes differ only by the chooser (the counts in the chips); T3 taken: exactly one chip pressed in the document.
3. The cookie is shared by the subdomains, an unknown deck id can arrive — closed with the code (`Decks::resolve` answers the words deck for an id this build does not teach) and T2 taken: `esperanto` and an empty value paint Words, no panic.
4. A dropped cookie resurrects the old key on the client — closed with reason: after the write-through it is one load in a cookie-blocked browser, where the theme and the language behave the same.
5. A refused write looks successful — closed with the code: the backend keeps a refused select in the page's memory (063a).
6. No `storage` event for a cookie — closed with reason: a second tab keeps its selection until reload, as the theme does.
T1 (store wins over the old key) not added: `DeviceStore` needs ui-lib's store context; the order is two lines.

## Merge
4b7e175 on main (squash of `fix/deck-selection-cookie`, tip 7625d47); prod tag v0.1.19. Review entry `review-073-deck-selection-cookie` archived; worktrees removed.

## Retro
No agents: one line. A device setting kept where the server cannot read it flashes on every load; the cookie store existed since the theme, the deck should have used it in 063a (a value captured at one time and read at another: design names the owner). Deep's write-through finding was the one that mattered.
