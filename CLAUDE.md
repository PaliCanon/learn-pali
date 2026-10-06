# CLAUDE.md — learn-pali

@../docs/README.md
@../docs/conventions/index.md
@../claude-memory/MEMORY.md

This repository: learning Pāli — a PaliCanon service, its scope defined
task by task in `.task/` before anything is built (conventions:
quality.md, "Define before building"). A workspace: `core` (the learning
loop, `learn_core`), `cli` (the loop at the terminal), `build_deck` (the
deck from the DPD) and `app` (the site: Leptos SSR + hydrate on ui-lib,
three screens, the progress in the browser). Build with the isolated
toolchain (`~/.rustup-pc`, see README.md).
