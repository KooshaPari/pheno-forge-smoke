# Changelog

All notable changes to `pheno-forge-smoke` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-06-24

### Added

- Initial release
- Single-binary CLI that loads `libpheno_bridge.dylib` via dlopen
- 4-scope end-to-end smoke (episodic / identity / project_knowledge / fallback)
- 3 modes: `mock` (CI dry-run), `sidecar` (live sidecars), `c` (compile-only parity)
- JSONL + human-readable report formats
- Sidecar health probes (`:3030`, `:8283`, `:8000`)
- Meta-bundle: README, AGENTS, WORKLOG v2.1, CHANGELOG, llms.txt, LICENSE-{MIT,APACHE}

### Refs

- ADR-096 — forgecode improvement (the locked stack)
- ADR-097 — eval harness (next wave)
- ADR-098 — additional adapters (next wave)
- `KooshaPari/pheno-forge-plugins` v0.1.0
- `KooshaPari/pheno-cdylib-bridge` v0.1.0
- `KooshaPari/thegent#1144` (thegent-memory v2)
- `tailcallhq/forgecode#3559` (forge_pheno_memory)