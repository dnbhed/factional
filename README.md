# Factional

A modular, single-player RPG engine in Rust. It's built in small, test-driven increments.

The first module is **reputation & factions**. It covers:

- each character's alignment, on a sliding lawful↔chaotic and good↔evil scale;
- how characters and factions regard each other;
- faction membership, rank and standing;
- how all of that changes as characters act and complete quests.

The engine knows nothing about maps, graphics or combat. Those modules come later, and plug in through commands, events and queries.

**Status:** the design is being agreed, and there's no code yet. The next increment is **F0** in [docs/PLAN.md](docs/PLAN.md).

| Document | What's in it |
| --- | --- |
| [docs/DESIGN.md](docs/DESIGN.md) | The model, the formulas, the API, the tuning surface, and the sample world |
| [docs/DECISIONS.md](docs/DECISIONS.md) | What's agreed, proposed, and still open |
| [docs/PLAN.md](docs/PLAN.md) | The queue of increments |
| [CLAUDE.md](CLAUDE.md) | How agents work in this repo |

Once F0 lands, open the sample world with:

```bash
cargo run -p factional-cli -- repl content/sample
```
