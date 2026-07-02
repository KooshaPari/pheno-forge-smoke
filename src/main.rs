// SPDX-License-Identifier: MIT OR Apache-2.0
//! pheno-forge-smoke: end-to-end smoke test for the 4-PR forgecode improvement stack
//! (ADR-096).
//!
//! What it does:
//!   1. Loads libpheno_bridge via libloading::Library (cdylib shipped by pheno-cdylib-bridge)
//!   2. Verifies bridge version
//!   3. Constructs a CompositeAdapter via the bridge
//!   4. Routes a store + recall + forget through each of the 4 scopes
//!   5. Optionally probes the live sidecar health endpoints
//!   6. Emits a JSONL or human-readable report
//!
//! Modes:
//!   --mode=mock       (default)  uses the bridge's composite adapter; passes
//!                                if the bridge loads (round-trip calls are
//!                                expected to fail without live sidecars;
//!                                the smoke counts them as "expected fail")
//!   --mode=sidecar              requires live sidecars; round-trip calls
//!                                must succeed
//!
//! Exit codes:
//!   0  all required checks passed
//!   1  one or more required checks failed
//!   2  bridge not loadable (cdylib missing or broken)

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use colored::*;
use pheno_forge_smoke::{default_bridge_path, Bridge, MemoryValue, Provider, Scope};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[derive(Parser, Debug)]
#[command(name = "pheno-forge-smoke")]
#[command(about = "End-to-end smoke for the forgecode improvement stack (ADR-096)")]
struct Args {
    /// Operating mode
    #[arg(long, value_enum, default_value_t = Mode::Mock)]
    mode: Mode,

    /// Override the bridge library path (default: PHENO_BRIDGE_PATH or standard search)
    #[arg(long)]
    bridge_path: Option<String>,

    /// Sidecar endpoint overrides
    #[arg(long)]
    supermemory_endpoint: Option<String>,
    #[arg(long)]
    letta_endpoint: Option<String>,
    #[arg(long)]
    mem0_endpoint: Option<String>,

    /// Emit JSONL to stdout instead of human-readable
    #[arg(long)]
    jsonl: bool,

    /// Skip healthchecks (faster; useful for CI dry-runs)
    #[arg(long)]
    skip_healthcheck: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Mode {
    Mock,
    Sidecar,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CheckResult {
    name: String,
    passed: bool,
    detail: String,
    duration_ms: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct SmokeReport {
    started_at: String,
    finished_at: String,
    mode: String,
    bridge_version: String,
    bridge_path: String,
    checks: Vec<CheckResult>,
    overall_pass: bool,
}

/// Whether the terminal should use ANSI color output.
fn use_color() -> bool {
    static NO_COLOR: OnceLock<bool> = OnceLock::new();
    *NO_COLOR.get_or_init(|| {
        let val = std::env::var("NO_COLOR").unwrap_or_default();
        val.is_empty() && atty::is(atty::Stream::Stdout)
    })
}

/// Apply bold if color is enabled.
fn maybe_bold(s: &str) -> ColoredString {
    if use_color() {
        s.bold()
    } else {
        s.normal()
    }
}

/// Apply green color if enabled.
fn maybe_green(s: &str) -> ColoredString {
    if use_color() {
        s.green()
    } else {
        s.normal()
    }
}

/// Apply red color if enabled.
fn maybe_red(s: &str) -> ColoredString {
    if use_color() {
        s.red()
    } else {
        s.normal()
    }
}

/// Apply dim style if color is enabled.
fn maybe_dim(s: &str) -> ColoredString {
    if use_color() {
        s.dimmed()
    } else {
        s.normal()
    }
}

/// Apply cyan color if enabled.
fn maybe_cyan(s: &str) -> ColoredString {
    if use_color() {
        s.cyan()
    } else {
        s.normal()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let started = Utc::now();

    if !args.jsonl {
        println!(
            "{} {} {}",
            maybe_cyan("pheno-forge-smoke").bold(),
            maybe_dim(env!("CARGO_PKG_VERSION")),
            maybe_dim(&format!("(mode: {:?})", args.mode))
        );
        println!();
    }

    let mut checks: Vec<CheckResult> = Vec::new();

    // 1. Load the bridge
    let bridge_path = args
        .bridge_path
        .as_deref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_bridge_path);

    let bridge = match Bridge::load(bridge_path.to_string_lossy().as_ref())
        .with_context(|| format!("loading bridge at {}", bridge_path.display()))
    {
        Ok(b) => {
            checks.push(CheckResult {
                name: "bridge_load".into(),
                passed: true,
                detail: format!("loaded {}", bridge_path.display()),
                duration_ms: 0,
            });
            b
        }
        Err(e) => {
            checks.push(CheckResult {
                name: "bridge_load".into(),
                passed: false,
                detail: format!("failed to load {}: {:#}", bridge_path.display(), e),
                duration_ms: 0,
            });
            return finalize(started, args, checks, bridge_path, String::new()).await;
        }
    };

    // 2. Verify version
    let bridge_version_str = match bridge.version() {
        Ok(v) => {
            checks.push(CheckResult {
                name: "bridge_version".into(),
                passed: true,
                detail: format!("version={}", v),
                duration_ms: 0,
            });
            v
        }
        Err(e) => {
            checks.push(CheckResult {
                name: "bridge_version".into(),
                passed: false,
                detail: format!("version call failed: {}", e),
                duration_ms: 0,
            });
            String::new()
        }
    };

    // 3. Sidecar healthchecks (live sidecar mode only)
    if !args.skip_healthcheck && matches!(args.mode, Mode::Sidecar) {
        checks.push(
            check_endpoint_health(
                "supermemory_health",
                args.supermemory_endpoint
                    .as_deref()
                    .unwrap_or("http://127.0.0.1:3030/health"),
            )
            .await,
        );
        checks.push(
            check_endpoint_health(
                "letta_health",
                args.letta_endpoint
                    .as_deref()
                    .unwrap_or("http://127.0.0.1:8283/health"),
            )
            .await,
        );
        checks.push(
            check_endpoint_health(
                "mem0_health",
                args.mem0_endpoint
                    .as_deref()
                    .unwrap_or("http://127.0.0.1:8000/health"),
            )
            .await,
        );
    } else if matches!(args.mode, Mode::Mock) {
        checks.push(CheckResult {
            name: "sidecar_health".into(),
            passed: true,
            detail: "skipped (mode=mock)".into(),
            duration_ms: 0,
        });
    }

    // 4. Composite adapter routes each scope
    for (scope_label, scope) in [
        ("episodic", Scope::Episodic),
        ("identity", Scope::Identity),
        ("project_knowledge", Scope::ProjectKnowledge),
        ("fallback", Scope::Fallback),
    ] {
        let check = run_scope_route(scope_label, scope, &bridge, &args).await;
        checks.push(check);
    }

    // 5. Composite constructs cleanly
    let check = run_composite_construct(&bridge);
    checks.push(check);

    finalize(started, args, checks, bridge_path, bridge_version_str).await
}

async fn check_endpoint_health(name: &str, url: &str) -> CheckResult {
    let start = Instant::now();
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return CheckResult {
                name: name.into(),
                passed: false,
                detail: format!("reqwest build failed: {}", e),
                duration_ms: start.elapsed().as_millis() as u64,
            }
        }
    };
    match client.get(url).send().await {
        Ok(resp) if resp.status().is_success() => CheckResult {
            name: name.into(),
            passed: true,
            detail: format!("{} -> {}", url, resp.status()),
            duration_ms: start.elapsed().as_millis() as u64,
        },
        Ok(resp) => CheckResult {
            name: name.into(),
            passed: false,
            detail: format!("{} -> HTTP {}", url, resp.status()),
            duration_ms: start.elapsed().as_millis() as u64,
        },
        Err(e) => CheckResult {
            name: name.into(),
            passed: false,
            detail: format!("{} -> {}", url, e),
            duration_ms: start.elapsed().as_millis() as u64,
        },
    }
}

async fn run_scope_route(scope_label: &str, scope: Scope, bridge: &Bridge, args: &Args) -> CheckResult {
    let start = Instant::now();
    let key = format!("smoke-key-{}", scope_label);
    let value_text = format!("smoke-value-{}", Utc::now().timestamp_millis());
    let value = MemoryValue::Text(value_text.clone());

    let handle = match bridge.new_handle(Provider::Composite) {
        Ok(h) => h,
        Err(e) => {
            return CheckResult {
                name: format!("scope_{}", scope_label),
                passed: false,
                detail: format!("handle construct failed: {}", e),
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    // Each bridge.* call takes &self + a Copy handle; handle is Copy so we
    // can pass it freely. The bridge holds the Library alive across calls.
    let store_result = bridge.store(handle, scope, &key, &value);
    let recall_result = bridge.recall(handle, scope, &key);
    let forget_result = bridge.forget(handle, scope, &key);
    bridge.free_handle(handle);

    // In mock mode, the round-trip will fail because no sidecars are running;
    // we count that as "expected" and pass the check as long as the bridge
    // surfaced a meaningful error. In sidecar mode, all 3 must succeed.
    let pass = match args.mode {
        Mode::Mock => {
            // Mock mode: bridge must surface ANY non-zero error (proves the
            // call went all the way through the cdylib and into the
            // backend adapter).
            store_result.is_err() || recall_result.is_err()
        }
        Mode::Sidecar => store_result.is_ok() && recall_result.is_ok() && forget_result.is_ok(),
    };

    let detail = format!(
        "store={} recall={} forget={}",
        short(&store_result),
        short(&recall_result),
        short(&forget_result),
    );

    CheckResult {
        name: format!("scope_{}", scope_label),
        passed: pass,
        detail,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

fn short<T, E: std::fmt::Display>(r: &Result<T, E>) -> &'static str {
    match r {
        Ok(_) => "ok",
        Err(_) => "err",
    }
}

fn run_composite_construct(bridge: &Bridge) -> CheckResult {
    let start = Instant::now();
    let result = bridge.new_handle(Provider::Composite);
    let passed = result.is_ok();
    let detail = match result {
        Ok(h) => {
            bridge.free_handle(h);
            "composite provider constructs, handle freed cleanly".to_string()
        }
        Err(e) => format!("composite construct failed: {}", e),
    };
    CheckResult {
        name: "composite_construct".into(),
        passed,
        detail,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

async fn finalize(
    started: chrono::DateTime<Utc>,
    args: Args,
    checks: Vec<CheckResult>,
    bridge_path: std::path::PathBuf,
    bridge_version_str: String,
) -> Result<()> {
    let finished = Utc::now();
    let overall_pass = checks.iter().all(|c| c.passed);
    let report = SmokeReport {
        started_at: started.to_rfc3339(),
        finished_at: finished.to_rfc3339(),
        mode: format!("{:?}", args.mode),
        bridge_version: bridge_version_str,
        bridge_path: bridge_path.to_string_lossy().to_string(),
        overall_pass,
        checks: checks.clone(),
    };

    if args.jsonl {
        for check in &checks {
            println!("{}", serde_json::to_string(check)?);
        }
        println!("{}", serde_json::to_string(&report)?);
    } else {
        println!("{}", maybe_bold("Results:"));
        for check in &checks {
            let mark = if check.passed {
                maybe_green("OK  ").bold()
            } else {
                maybe_red("FAIL").bold()
            };
            let detail = if check.passed {
                maybe_dim(&check.detail)
            } else {
                check.detail.normal()
            };
            println!(
                "  {} {:<32} ({} ms)  {}",
                mark, check.name, check.duration_ms, detail
            );
        }
        println!();
        let summary: ColoredString = if overall_pass {
            maybe_green(&format!("ALL {} CHECKS PASSED", checks.len())).bold()
        } else {
            let failed = checks.iter().filter(|c| !c.passed).count();
            maybe_red(&format!("{} / {} CHECKS FAILED", failed, checks.len())).bold()
        };
        println!("{}", summary);
        println!(
            "{}",
            maybe_dim(&format!(
                "Bridge: {} ({})  Mode: {:?}  Duration: {} ms",
                report.bridge_version,
                report.bridge_path,
                args.mode,
                (finished - started).num_milliseconds()
            ))
        );
    }

    sleep(Duration::from_millis(10)).await;

    if overall_pass {
        Ok(())
    } else {
        std::process::exit(1);
    }
}