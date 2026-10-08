# Factional

A modular, single-player RPG engine in Rust. It's built in small, test-driven increments.

The first module is **reputation & factions**. It covers:

- each character's alignment, on a sliding lawful↔chaotic and good↔evil scale;
- how characters and factions regard each other;
- faction membership, rank and standing;
- how all of that changes as characters act and complete quests.

The second module, **quests**, is being built. Quests have stages with choices, chain into questlines, and are checked so that no quest can quietly shut another off. Today it reads and checks quest content, every lockout included. Playing quests comes later.

The engine knows nothing about maps, graphics or combat. Those modules come later, and plug in through commands, events and queries.

**Status:** early. [docs/PLAN.md](docs/PLAN.md) lists what's done and what's next.

| Document | What's in it |
| --- | --- |
| [docs/DESIGN.md](docs/DESIGN.md) | The model, the formulas, the API, the tuning surface, and the sample world |
| [docs/DECISIONS.md](docs/DECISIONS.md) | What's agreed, proposed, and still open |
| [docs/PLAN.md](docs/PLAN.md) | The queue of increments |
| [scenarios/README.md](scenarios/README.md) | Writing scenario scripts |
| [CLAUDE.md](CLAUDE.md) | How agents work in this repo, including the checks every change must pass |

## Getting started

1. Install Rust with [rustup](https://rustup.rs). The exact version is pinned in `rust-toolchain.toml`; install it once from the repo's root:

   ```bash
   rustup toolchain install
   ```

2. Install the two test tools, for snapshot review and mutation testing:

   ```bash
   cargo install --locked cargo-insta cargo-mutants
   ```

3. Start an interactive session; `help` lists the commands:

   ```bash
   cargo run -p factional-cli -- repl
   ```

   Then type `load content/sample` to load Riverhold, the sample world, and `characters` to see who's in it.

4. Or replay a scenario script:

   ```bash
   cargo run -p factional-cli -- run scenarios/smoke.scenario
   ```
