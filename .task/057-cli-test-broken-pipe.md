# 057 The CLI test harness tolerates a program that exits before reading its input

status: merged · rank: 1 · level: simple · type: fix · request: found by the orchestrator, 2026-10-17 (CI on main 34c489c red: learn_cli test a_wrong_deck_warns_about_the_terms_it_does_not_know, BrokenPipe at cli/tests/cli.rs:72) · light path

## Summary
Rank: difficulty 1 (one test helper), impact 2 (main's CI red, blocking the prod tag of 054) → 1.
The harness writes the piped answers to the child's stdin with `.unwrap()`; with a wrong deck the CLI warns and exits before reading, so on a fast runner the write fails with BrokenPipe and the test panics before reading the child's output. Fix: a BrokenPipe on the stdin write is expected (the program did not need the input): ignore that error kind only, keep every other error a failure; the assertions on code, out and err stay. Reproduce the race locally first (e.g. a large input that cannot fit the pipe buffer, or a sleep before the write in the test), then fix.

## Log
| Step | Result | Output |
| --- | --- | --- |
| Build | done | e8ceeb9; red: `called 'Result::unwrap()' on an 'Err' value: Os { code: 32, kind: BrokenPipe, message: "Broken pipe" }` at cli/tests/cli.rs:72; green: 5 runs all exit 0, clippy 0, fmt 0 |
| Check | done | orchestrator: diff read (BrokenPipe on the stdin write ignored, any other error panics; a 200 KB wrong-deck test) |
| Merge | done | a406129 on main, pushed |

## Retro
One line (light path): a test that pipes input to a child that may exit early must tolerate BrokenPipe; the race showed only on CI's faster runner.
