# the book

start with [a board, from nothing](01-a-board.md): it builds the smallest example up one step at a time, from a net to a routed, rendered board, and links to everything else.

[the examples](02-examples.md): one board per idea, from four LEDs to a complete devboard. each one is a crate you can build, preview, and `cargo test`.

## reference

- [the command line](reference/cli.md): every `bb` command and flag, what each writes to `output/`, and the tools it calls
- [the sdk](reference/sdk.md): every builder a board is written with, by what it makes
- [releasing a board](reference/release.md): the approved parts list, the review lock, and what `check`, `export` and `prep` produce
- [under the hood](reference/internals.md): the sandbox, the WIT contract, and the browser playground
