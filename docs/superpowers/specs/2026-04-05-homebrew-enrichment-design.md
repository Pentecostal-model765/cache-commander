# Homebrew Cache Enrichment Design

**Date:** 2026-04-05
**Status:** Draft
**Scope:** Enrich the Homebrew provider with semantic name parsing, bottle manifest metadata, and background `brew audit` integration.

## Context

The Homebrew provider (`src/providers/homebrew.rs`) currently handles only 4 static directory names (`downloads`, `Cask`, `api`, `bootsnap`). The actual Homebrew cache at `~/Library/Caches/Homebrew/` contains rich per-package data:

- **Top-level symlinks** like `awscli--2.34.24` pointing to bottles in `downloads/`
- **Bottle manifest JSONs** containing license, runtime dependencies, installed size, and architecture info
- **Cask downloads** in the `Cask/` subdirectory

Other providers (npm, pip, cargo) already parse individual packages and feed into the security scanning pipeline. Homebrew lags behind.

**Decision:** No OSV.dev integration — Homebrew bottles are prebuilt binaries and don't map to OSV ecosystems. Instead, we use `brew audit` as the native security/quality check.

## Design

### 1. Semantic Name Parsing

Extend `semantic_name()` to parse Homebrew cache file patterns:

| Input pattern | Output |
|---|---|
| `awscli--2.34.24` (bottle symlink) | `[bottle] awscli 2.34.24` |
| `awscli_bottle_manifest--2.34.24` (manifest symlink) | `[manifest] awscli 2.34.24` |
| Files inside `Cask/` | `[cask] <name>` |
| `downloads`, `Cask`, `api`, `bootsnap` | Existing labels unchanged |

**Parsing logic:** Split on `--` (double-dash) to separate name from version. Names may contain single hyphens (e.g. `json-c--0.18`). For manifests, strip the `_bottle_manifest` suffix from the name portion. Version may contain underscores for revisions (e.g. `2.53.0_1`).

### 2. Rich Metadata from Bottle Manifests

When `metadata()` is called on a bottle entry:

1. Derive the companion manifest path: sibling symlink named `<name>_bottle_manifest--<version>`
2. Read and parse the manifest JSON
3. Extract from the OCI manifest annotations:
   - `sh.brew.license` → License field
   - `sh.brew.bottle.installed_size` → Installed size (formatted as human-readable)
   - `sh.brew.tab` (embedded JSON string) → runtime dependency count and direct dependency names
   - `platform.architecture` + `platform.os` → Architecture field

When called on a manifest entry directly, extract the same data from the file itself.

When called on `downloads/`, count bottles vs manifests and show totals.

**Error handling:** Missing or malformed manifests silently skip affected fields. Same pattern as npm's `detect_install_scripts`.

**No new dependencies:** Hand-rolled JSON field extraction using `find()` + slice parsing, matching the existing pattern in `npm.rs`.

### 3. Background `brew audit` Integration

New scanner pipeline variants:

- `ScanRequest::BrewAudit` — triggered when Homebrew cache roots are detected
- `ScanResult::BrewAuditCompleted(HashMap<String, Vec<String>>)` — maps formula name to audit warnings

**Background execution:**
- Spawns `brew audit --installed` on a background thread
- Parses stdout format: `<formula>:\n  * <warning>\n  * <warning>`
- Sends results through existing `mpsc` channel
- Detail panel renders warnings alongside existing security info

**Guard rails:**
- Only triggered when Homebrew roots are present
- If `brew` is not on `$PATH`, skip silently (no error state)
- Parsing function is a pure `fn(&str) -> HashMap<String, Vec<String>>` for testability

### 4. No `package_id()` Implementation

Homebrew does not integrate with OSV.dev or registry version checking. The `package_id()` match arm in `mod.rs` remains `_ => None` for `CacheKind::Homebrew`. The `upgrade_command()` match also remains `None`.

## Testing Strategy

**TDD approach:** Tests are written first, implementation follows.

### Unit tests in `homebrew.rs`

**`semantic_name` parsing:**
- Bottle symlink: `awscli--2.34.24` → `[bottle] awscli 2.34.24`
- Manifest symlink: `awscli_bottle_manifest--2.34.24` → `[manifest] awscli 2.34.24`
- Hyphenated package name: `json-c--0.18` → `[bottle] json-c 0.18`
- Version with revision suffix: `git--2.53.0_1` → `[bottle] git 2.53.0_1`
- Existing directory names (`downloads`, `Cask`, `api`, `bootsnap`) still return their current labels
- Unknown names return `None`
- Cask files: entries inside Cask directory

**`metadata` manifest extraction:**
- License extraction from fixture JSON
- Runtime dependency count and direct dep names
- Installed size formatting
- Architecture extraction
- Missing `sh.brew.tab` field → skip gracefully
- Missing fields within tab → skip gracefully
- Malformed manifest JSON → return empty metadata
- Empty manifest file → return empty metadata
- Manifest with no matching platform entry

**`parse_brew_audit` output parsing:**
- Single formula with one warning
- Single formula with multiple warnings
- Multiple formulae
- Clean audit (empty output) → empty map
- Malformed/unexpected output → best-effort parse, no panic

### Filesystem fixture tests (using `tempfile`)

- Create fake Homebrew cache tree with symlinks and manifest files
- Verify `semantic_name` resolves correctly on real filesystem
- Verify `metadata` follows symlink to read manifest content

### Integration tests in `scanner/mod.rs`

- `BrewAudit` request/result round-trip through channel
- Missing `brew` binary → no result sent, no panic

### Regression tests in `providers/mod.rs`

- `detect()` still returns `CacheKind::Homebrew` for Homebrew paths
- `semantic_name` dispatch reaches new homebrew logic

## Files Modified

| File | Changes |
|---|---|
| `src/providers/homebrew.rs` | Extended `semantic_name`, `metadata`, new `parse_brew_audit` |
| `src/providers/mod.rs` | No changes needed (dispatch already wired) |
| `src/scanner/mod.rs` | New `BrewAudit` request/result variants, trigger logic |
| `src/security/mod.rs` | No changes — brew audit results stored as `HashMap<String, Vec<String>>` directly in app state |
| `src/ui/detail_panel.rs` | Render brew audit warnings |
| `src/app.rs` | Store brew audit results, trigger scan |
