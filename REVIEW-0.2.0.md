# Pre-0.2.0 Release Review

**Date:** 2026-04-06
**Reviewer:** Claude (automated deep review — code, tests, CI)
**Tests:** 707 passing, cargo-deny clean, cargo fmt clean

## CRITICAL — Fix Before Tagging 0.2.0

### C1. Mutex/Arc double-unwrap panic risk
**`src/security/mod.rs:121,131,151,168`**

If any HTTP thread panics, the mutex gets poisoned, then `lock().unwrap()` and `Arc::try_unwrap().unwrap().into_inner().unwrap()` cascade-crash the whole app. This is the only code path where a network failure could crash ccmd.

### C2. Hardcoded User-Agent `ccmd/0.1` with wrong repo URL
**`src/security/osv.rs:57,195` and `src/security/registry.rs:27`**

All three say `ccmd/0.1 (https://github.com/ccmd)` — wrong URL and wrong version. Should use `env!("CARGO_PKG_VERSION")` and the real repo URL.

### C3. Missing `rust-version` in Cargo.toml
README says "Rust 1.85+" but nothing enforces it.

## HIGH — Should Fix Before Release

### H1. `upgrade_command` doesn't sanitize shell metacharacters
**`src/providers/mod.rs:158-166`**

Package names from the filesystem are interpolated into shell commands. A crafted package name containing `; rm -rf /` produces a dangerous clipboard payload.

### H2. `brew outdated` exit status not checked
**`src/scanner/mod.rs:162-172`**

`run_brew_outdated()` doesn't check `output.status.success()`. A failing `brew` command's stdout gets parsed as if valid.

### H3. `parse_npm_latest` returns `Some("")` for empty version strings
**`src/security/registry.rs:13`**

Empty string versions flow into `compare_versions("1.0.0", "")` which incorrectly marks everything as outdated.

## MEDIUM — Improve For Quality

### M1. OSV batch response index alignment is fragile
**`src/security/mod.rs:61-63`** — assumes 1:1 mapping without length validation.

### M2. `let _ = self.scan_tx.send(...)` everywhere in `app.rs`
~10 call sites silently ignore scanner thread crashes.

### M3. Status messages overwrite each other
**`src/app.rs:120,141,154`** — rapid completion clobbers earlier messages.

### M4. UTF-8 unsafe byte-level parsing in Homebrew provider
**`src/providers/homebrew.rs:112-122`** — raw byte slicing on JSON.

### M5. `SafetyLevel::Unsafe` variant is dead code
**`src/providers/mod.rs:29`** — defined but never returned.

### M6. CI clippy only runs default lints
**`.github/workflows/ci.yml:54`** — 80+ pedantic findings uncaught.

## TEST COVERAGE GAPS

| Gap | Priority |
|---|---|
| No tests for `perform_delete` error paths | High |
| No mocked HTTP tests for OSV/registry | High |
| No tests for scanner threading | High |
| No tests for `tick()` state machine | Medium |
| `osv_query_finds_urllib3_vulns` is network-dependent | Medium |

## CI PIPELINE

**Solid:** SHA-pinned actions, minimal permissions, tag validation, provenance attestation, cargo-deny.
**Gaps:** No MSRV enforcement, no cross-compiled binary smoke test, no `cargo test --doc`.
