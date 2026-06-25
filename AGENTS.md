# AGENTS.md — pheno-forge-smoke

**Status:** ACTIVE (single-purpose CLI; this file is the canonical guide for agents working on this repo).

---

## Project Overview

`pheno-forge-smoke` is a single-binary end-to-end smoke test for the 4-PR forgecode improvement sequence (ADR-096). It dlopens `libpheno_bridge.dylib` (the C-ABI shared lib from `pheno-cdylib-bridge`) and exercises the full memory stack: supermemory + letta + cognee + mem0 via the `CompositeAdapter`.

It is the **first PR in the post-merge follow-up wave** (ADR-097 + ADR-098). The eval harness (ADR-097) consumes this smoke binary as its entry point for per-run regression detection.

---

## Stack

- **Language:** Rust (1.85+, edition 2021)
- **Crate type:** `lib` + `bin` (the lib is a thin C-ABI wrapper; the bin is the CLI)
- **FFI:** direct `extern "C"` declarations of the 8 `pheno_*` symbols; `dlopen`/`dlsym` resolved at runtime by the platform loader.
- **Async runtime:** tokio 1.49 (full features)

---

## Build

```bash
# Requires pheno-cdylib-bridge to be built first (the .dylib is dlopened at runtime)
cd ../pheno-cdylib-bridge && cargo build --release
cd ../pheno-forge-smoke && cargo build --release
```

Outputs:
- `target/release/pheno-forge-smoke` — the binary
- `target/release/libpheno_forge_smoke.rlib` — the lib

---

## Conventions

- **Branch naming:** `chore/<req-id>-<slug>-<date>` for chore work; `feat/<req-id>-<slug>-<date>` for features
- **Commit messages:** Conventional Commits (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`, `build:`, `ci:`) with optional scope
- **PR labels:** `governance` for cleanup, `L<n>-#<n>` for tracking against DAG level
- **SOTA artifacts:** `findings/`, `docs/adr/<date>/` (lives in the monorepo, not here)
- **Meta-bundle for a release-ready crate:** `AGENTS.md` + `llms.txt` + `WORKLOG.md` + `CHANGELOG.md` + `LICENSE-MIT`

---

## Quality bar (per ADR-023 Rule 3.1)

This crate is a CLI tooling utility, not a `pheno-*-lib` / `phenotype-*-sdk` / `phenotype-*-framework`. It still ships with:

- Spec: this file + README
- Tests: `cargo test`
- Coverage gate: 60% (CLI tooling tier per ADR-040)
- Observability: `tracing` + `tracing-subscriber` (info-level minimum, OTLP export via `pheno-tracing` deferred to v0.2)
- Worklog: `WORKLOG.md` (v2.1 schema w/ `device:` field)

---

## Related

- `README.md` — usage + examples
- `WORKLOG.md` — worklog v2.1 (with `device:` field per ADR-015 v2.1)
- `CHANGELOG.md` — release history
- `llms.txt` — LLM-friendly summary
- `LICENSE-MIT`, `LICENSE-APACHE` — dual license

---

## Refs (governance)

- **ADR-096** — forgecode improvement (the locked stack this smoke tests)
- **ADR-097** — eval harness design (consumes this smoke as its entry point)
- **ADR-098** — additional adapters (next wave; this smoke is the regression baseline)