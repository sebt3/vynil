# Vynil — Agent Guide

Vynil is a package manager for Kubernetes (a dpkg/rpm equivalent for the cluster), written in
Rust, with Handlebars templates and Rhai scripts. See [`docs/architecture.md`](docs/architecture.md)
for the component model.

This project is governed by **SpecDD**. Before doing any work:

1. Read [`.specdd/bootstrap.md`](.specdd/bootstrap.md).
2. Read [`.specdd/bootstrap.project.md`](.specdd/bootstrap.project.md) for project rules.
3. Resolve and read the `.sdd` spec chain that covers your target.

`.sdd` files are binding development contracts, not documentation. Follow the SpecDD loop:
**Resolve → Read → Authorize → Change → Verify → Report.** Do not jump straight to code.

## How this project specifies code

- One `.sdd` per source file, co-located as a same-basename spec
  (`common/src/vynilpackage.rs` ↔ `common/src/vynilpackage.sdd`). Group two or three files only
  with a written justification.
- A file spec fully describes the file: `Purpose`, `Exposes`, `Accepts`, `Returns`, `Raises`,
  `Handles`, `Must`, and `Scenario`. Directory and crate specs stay coarse and defer to them.
- Specs are committed and reviewed like code. The retired `docs/conception/` + `.tasks/`
  workflow no longer exists — do not use it as authority.

## Test-driven specifications (TTD)

No behaviour change ships without tests written from the spec's `Scenario` sections first, then
the code. The code satisfies the tests. Move a spec `Tasks` entry to `[x]` only once every check
below is green.

## Verification

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo +nightly fmt --check`
- `specdd lint`

The strict static-analysis harness (`pedantic` denied, `nursery` warned, and product-code
`unwrap()`/`expect()` denied per crate) is defined in [`linting.sdd`](linting.sdd) and enforced
from `Cargo.toml` and `rustfmt.toml`.

## Agents

The team works through these opencode agents (in [`.opencode/agent/`](.opencode/agent)):

| Agent | Role |
|---|---|
| `spec-driver` | Primary. Co-authors `.sdd` specs with the developer and orchestrates the others. Writes no code. |
| `implementer` | Realizes one approved spec test-first; keeps clippy/fmt/tests green; edits no `.sdd`. |
| `validator` | Read-only. Checks code and tests against the spec and the harness; returns a deviation report. |
| `reverse-engineer` | Reads one source file and drafts its most complete spec; writes only that `.sdd`. |

Start work by engaging the `spec-driver`; it delegates implementation, validation, and reverse
engineering. Do not chain slices automatically — one behaviour at a time, verify, then report and
wait for the developer.

## Git

- Branch from `origin/main`: `feat/<name>` or `fix/<short-desc>`.
- Commit first line `feat: …` or `fix: …`; explain the why in the body.
