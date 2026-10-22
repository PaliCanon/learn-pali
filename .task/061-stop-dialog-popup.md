# 061 Learn on ui-lib v0.4.0: the phone menu button, 'Stop here?' as a popup

status: merged · rank: 2 · level: simple · type: feat · request: the user, 2026-10-22 (phone check: «Les écrans comme stop here devraient être un pop up», the menu button, the aligned rows) · after: [ui-lib 070] · light path

## Summary
Rank: difficulty 2 (a dependency bump and one prop), impact 2 (every phone visitor of Learn) → 2.
ui-lib v0.4.0 (070) brings the phone bar's menu button, the aligned navigation rows and `DialogSize::Popup`. Learn bumps its tag from v0.3.2 to v0.4.0 (app/Cargo.toml, `cargo update -p palicanon-ui-lib`) and its StopDialog (app/src/components/session.rs ~l.403) sets `size=DialogSize::Popup` and drops `layout="sm:max-w-md"` (Popup is max-w-md itself); a render test asserts the Stop dialog's panel carries `max-w-md` and no `h-full`, and exactly one `max-w-` utility; the existing Escape and stop tests stay green. The Settings dialog stays a Sheet. Check: phone captures of the bar, the open menu and 'Stop here?' on the task server (CDP 390 px, scrollWidth == innerWidth), then the user.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | 4ffb525 bump v0.4.0 (lock 0.4.0, one ui-lib in cargo tree -d); a976929 StopDialog size=DialogSize::Popup. Red: the_stop_dialog_is_a_popup_not_a_sheet failed at max-w-md (panel held sm:max-w-[40rem] + sm:max-w-md, h-full); mutation Popup to Sheet red again. Checks: test exit 0 (525 lib passed), clippy ssr/csr/hydrate exit 0, fmt exit 0, leptosfmt exit 0, cargo leptos build exit 0. Captures 390x844 CDP, scrollWidth 390 == innerWidth 390 (home, menu, stop): home-bar, menu-open, stop-here PNGs; server 127.0.0.1:3061 |
| Check | done | the user, 2026-10-22, on the captures (stop-here.png, menu-open.png, home-bar.png; scrollWidth == innerWidth 390): «Oui, fusionne» (branch tip c6e25cd, ui-lib v0.4.0) |
| Merge | done | b884105 on main, pushed (dev deploys) |

## Retro
One line (light path): the brief quoted a ui-lib tag (v0.3.2) the file did not hold (v0.3.1): a bump brief reads the file first; `cargo leptos watch` is restarted after a build before a capture (058 again).
