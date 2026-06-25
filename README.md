# pheno-forge-smoke

End-to-end smoke test for the **4-PR forgecode improvement stack** (ADR-096,
accepted 2026-06-23).

```
user (forge CLI)
  -> pheno-forge-plugins sidecars (supermemory, letta, cognee, mem0, config, tracing)
  -> pheno-cdylib-bridge (C-ABI shared lib)
  -> thegent-memory v2 polyglot facade (MemoryPort trait + CompositeAdapter)
  -> forge_pheno_memory (upstream forgecode workspace crate)
         \-- this smoke binary sits at the bridge entry-point and verifies
              every layer below the user-facing CLI works.
```

## What it checks

1. **`bridge_load`** — `libpheno_bridge.dylib` is dlopen-able; `pheno_bridge_version()` returns the expected semver.
2. **`sidecar_health`** — (live-sidecar mode only) probes `:3030/health`, `:8283/health`, `:8000/health` (the systemd unit ports from `pheno-forge-plugins`).
3. **`scope_episodic`** — store + recall + forget through `CompositeAdapter(scope=Episodic)`.
4. **`scope_identity`** — same, `scope=Identity`.
5. **`scope_project_knowledge`** — same, `scope=ProjectKnowledge`.
6. **`scope_fallback`** — same, `scope=Fallback`.
7. **`composite_construct`** — `CompositeAdapter` opens and closes cleanly.

## Modes

| Mode | What it does | Use case |
|---|---|---|
| `--mode=mock` *(default)* | Loads the bridge; route calls are expected to fail (no sidecars running); passes if errors surface correctly. | CI dry-runs, dev machines without sidecars. |
| `--mode=sidecar` | Loads the bridge; probes sidecar health; route calls must succeed. | Pre-flight check before a real forge session. |
| `--mode=c` | Compile-only parity with the C smoke in `pheno-cdylib-bridge/c/examples/smoke.c`. | Test rig parity. |

## Build

```bash
# 1. Build the bridge (cdylib it will dlopen at runtime)
cd ../pheno-cdylib-bridge
cargo build --release
# -> target/release/libpheno_bridge.dylib (macOS)
#    target/release/libpheno_bridge.so   (Linux)

# 2. Build the smoke binary
cd ../pheno-forge-smoke
cargo build --release
# -> target/release/pheno-forge-smoke
```

## Run

```bash
# Default: mock mode, no sidecars required
./target/release/pheno-forge-smoke

# Sidecar mode: requires pheno-forge-sidecars.target to be up
./scripts/run-smoke.sh sidecar

# JSONL output (for CI parsing)
./target/release/pheno-forge-smoke --jsonl

# Skip healthchecks
./target/release/pheno-forge-smoke --mode=mock --skip-healthcheck
```

## Exit codes

- `0` — all required checks passed.
- `1` — one or more required checks failed.
- `2` — bridge not loadable (cdylib missing).

## Tests

```bash
cargo test
```

## License

Apache-2.0 OR MIT (your choice). See `LICENSE-APACHE` and `LICENSE-MIT`.

## Refs

- **ADR-096**: forgecode improvement (the locked stack)
- **ADR-097**: eval harness design (next wave)
- **ADR-098**: additional adapters (graphiti, hipporag — next wave)
- `KooshaPari/pheno-forge-plugins` v0.1.0
- `KooshaPari/pheno-cdylib-bridge` v0.1.0
- `KooshaPari/thegent#1144` (thegent-memory v2)
- `tailcallhq/forgecode#3559` (upstream forge_pheno_memory)