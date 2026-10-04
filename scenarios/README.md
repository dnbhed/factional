# Scenarios

Each `*.scenario` file is a scripted session with the `factional` CLI. Scenarios do two jobs:

- **Manual testing.** `cargo run -p factional-cli -- run scenarios/smoke.scenario` runs one and prints its transcript.
- **Regression tests.** `cargo test` runs every scenario and compares its transcript with a snapshot in `crates/cli/tests/snapshots/`.

## Format

- One command per line. Blank lines and lines starting with `#` are skipped.
- `assert <command> == <expected>` runs `<command>` and passes only if its result is exactly `<expected>`.
  - A command that fails has the result `error: <message>`, so failures can be asserted too.
  - The line splits at the first ` == `. A line ending in ` ==` expects an empty result.
- `quit` ends the script early.
- Paths, as in `load content/sample`, are relative to the current directory. `cargo test` runs scenarios from the repository root.
- `help` lists every command.

The run stops at the first line that goes wrong, and reports it as `line N: …`:

| What went wrong | Report |
| --- | --- |
| An unknown command, or an `assert` without ` == ` | `line N: unknown command 'frobnicate'` |
| An `assert` whose result differs | `line N: expected 'bye', got 'hi'` |
| A command that fails outside an `assert` | `line N: error: boom` |

## Snapshots

- A transcript lists each command after `> `, followed by its output. Comments and blank lines aren't included.
- A new or changed transcript fails `cargo test`. Run `cargo insta review` to see the difference.
- Read every difference before accepting it. An unexplained change is a bug until it's explained.
- Use `assert` lines for what the scenario is about. The snapshot is there to catch knock-on changes.
