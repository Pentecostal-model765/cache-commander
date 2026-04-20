# SwiftPM & Xcode DerivedData Provider Support

**Date:** 2026-04-20
**Issues:** [#11](https://github.com/juliensimon/cache-commander/issues/11), [#17](https://github.com/juliensimon/cache-commander/issues/17)
**Status:** Design

## Summary

Add two macOS-centric cache providers: **SwiftPM** (Swift Package Manager's global cache of git clones, artifacts, and manifests) and **Xcode** (DerivedData, iOS DeviceSupport, CoreSimulator Caches). Both are disk-hygiene-focused — neither participates in OSV vulnerability scanning or version-check registries. DerivedData is frequently the single largest cache on macOS developer machines (50–200 GB is typical), so correct detection and safe-deletion classification is the primary user value.

## Approach

Two independent providers, one PR. Both providers are additive: no changes to existing scanner, detection, or UI logic. Both ship under hardcore TDD — every function has at least one red→green test cycle, written before implementation.

## Provider: SwiftPM (`src/providers/swiftpm.rs`)

### Detection

`detect(path)` returns `CacheKind::SwiftPm` when any path component is `org.swift.swiftpm`, OR when an ancestor is `org.swift.swiftpm` (catches deep paths into `repositories/`, `artifacts/`, `manifests/`).

Canonical roots:
- `~/Library/Caches/org.swift.swiftpm/` (macOS)
- `~/.cache/org.swift.swiftpm/` (Linux)

### Package Identification

**None.** `package_id()` returns `None` unconditionally.

Rationale (per brainstorming):
- Version extraction from `repositories/<name>-<hash>/` requires parsing git refs, which is brittle and depends on resolver state we don't control.
- OSV's `SwiftURL` ecosystem has sparse coverage; implementing identity would add complexity for little scanning value.
- Disk hygiene is the primary user value. Identity can be added in a future PR once the `artifacts/` layout and OSV coverage mature.

Follow-up tracking issue to be filed after this PR lands.

### Semantic Names

- `repositories/<pkg>-<8-hex-hash>/` → `<pkg>` (strip trailing `-<8hex>` hash suffix). Example: `swift-collections-a1b2c3d4` → `swift-collections`.
- `artifacts/<pkg>/` → `<pkg>` (plain passthrough when the component is directly under `artifacts/`).
- `manifests/<filename>` → `None` (filenames are internal hashes, no meaningful display).
- The three known root-subdir names themselves (`repositories`, `artifacts`, `manifests`) → `None` (let the tree show them literally).

Hash detection rule: a trailing segment is treated as a hash iff it is `-` followed by 7 or more lowercase-hex characters at the end of the component. Exact minimum length is pinned by tests against real SwiftPM cache fixtures during TDD. False positives on real package names (e.g. a package literally named `x-abcdefg`) are acceptable since stripping still yields a human-readable name.

### Metadata

`metadata()` returns contextual labels for the three top-level subdirs:
- `repositories/` → `Contents: Git clones of package sources (re-cloneable)`
- `artifacts/` → `Contents: Binary artifacts (re-downloadable)`
- `manifests/` → `Contents: Cached Package.swift resolutions`

No metadata for leaf files.

### Safety

- `repositories/` subtree → `SafetyLevel::Caution` (deleting forces a full re-clone of every dependency; slow on first rebuild).
- `artifacts/` subtree → `SafetyLevel::Safe`.
- `manifests/` subtree → `SafetyLevel::Safe`.
- Anything else under `org.swift.swiftpm/` (future subdirs we don't know about yet) → `SafetyLevel::Caution` (conservative default).

Classification uses `has_adjacent_components(path, "org.swift.swiftpm", "repositories")` (L1-safe component matching), not substring checks.

### Upgrade Command

`None`. Swift package upgrades are project-local operations on `Package.swift` / `Package.resolved`, not on global cache entries. Documented in CHANGELOG so users aren't surprised the `c` key is a no-op for SwiftPM entries.

### Registry / OSV

Neither `build_registry_url` nor `check_latest` needs an arm. `package_id` returns `None`, so the scanner never queries either pipeline for SwiftPM.

## Provider: Xcode (`src/providers/xcode.rs`)

Single provider, three distinct root paths. Name chosen to reflect the umbrella (Xcode ecosystem), not just one subdir.

### Detection

`detect(path)` returns `CacheKind::Xcode` when `has_adjacent_components` (L1-safe) matches any of these pairs anywhere in `path.ancestors()`:

- `Xcode/DerivedData` (covers the `~/Library/Developer/Xcode/DerivedData/` subtree and deep paths within it)
- `Xcode/iOS DeviceSupport`
- `CoreSimulator/Caches`

No unqualified direct-name arm for `DerivedData` or `Caches` alone — those names are too ambiguous and would collide with unrelated directories. The adjacent-components requirement is the whole guard.

### Package Identification

**None.** No ecosystem, no OSV coverage. Pure disk hygiene.

### Semantic Names

**DerivedData:** For a path `.../DerivedData/<Project>-<hash>/` (top-level project directory):
1. Open `Info.plist` inside that directory.
2. Extract the `WORKSPACE_PATH` string value.
3. Display as `<basename of WORKSPACE_PATH> (at <full WORKSPACE_PATH>)`.

Example: if `Info.plist` contains `<key>WORKSPACE_PATH</key><string>/Users/j/dev/MyApp/MyApp.xcworkspace</string>`, the semantic name is `MyApp.xcworkspace (at /Users/j/dev/MyApp/MyApp.xcworkspace)`.

Fallback when `Info.plist` is missing, unreadable, or lacks `WORKSPACE_PATH`: return the directory name as-is (e.g. `MyApp-abc123def`). Never panic, never propagate errors.

**Info.plist parsing:** XML format only (Xcode consistently writes XML, not the binary variant). Simple string search for the `WORKSPACE_PATH` `<key>` / `<string>` pair — no added dependency on a plist crate. Must use `chars()`-based handling if slicing (L2, L5 guard).

**iOS DeviceSupport:** Leaf directories are named `<iOS-version> (<build>)` (e.g. `17.4 (21E213)`). Use the directory name directly as the semantic name.

**CoreSimulator/Caches:** No meaningful semantic name — directory layout is opaque. Return `None`.

### Metadata

`metadata()` returns root-level labels:
- `DerivedData/` → `Contents: Xcode build products and indexes (rebuildable, 5–30 min cost)`
- `iOS DeviceSupport/` → `Contents: Symbol files for connected iOS devices (re-downloadable on device reconnect)`
- `CoreSimulator/Caches/` → `Contents: iOS Simulator caches (safe to clear)`

For a DerivedData project dir, additionally emit the `WORKSPACE_PATH` as a metadata field when available, so the user can confirm which project they're about to nuke before confirming the delete.

### Safety

- `DerivedData/` subtree → `SafetyLevel::Caution` (rebuild is real and slow; matches Gradle's transform/build-cache classification).
- `iOS DeviceSupport/` subtree → `SafetyLevel::Safe` (re-downloaded automatically on next device connect).
- `CoreSimulator/Caches/` subtree → `SafetyLevel::Safe`.

Classification via `has_adjacent_components`. Explicit test `safety_rejects_confusable_suffix` covers `DerivedData-backup/`, `DerivedDatabase/`, `iOS DeviceSupport-old/` and asserts they are NOT classified as the real subtree.

### Upgrade Command

`None`. These are build artifacts, not package entries.

### Registry / OSV

Neither applies. No changes to `registry.rs` or `osv.rs`.

## Dispatch Integration (`src/providers/mod.rs`)

Add `pub mod swiftpm;` and `pub mod xcode;` declarations (preserving alphabetical order: `swiftpm` after `pre_commit`/`prisma`, `xcode` after `whisper`/`yarn`).

Add `CacheKind::SwiftPm` and `CacheKind::Xcode` arms to:

- `detect()` — direct-name + ancestor-walk arms as described above.
- `semantic_name()` — dispatch to `swiftpm::semantic_name` / `xcode::semantic_name`.
- `metadata()` — dispatch to `swiftpm::metadata` / `xcode::metadata`.
- `package_id()` — **no arm added**; they fall through to `_ => None` (documented in the match as a comment).
- `upgrade_command()` — **no arm added**; same fall-through.
- `safety()` — dedicated arms for both (replaces the default `Safe`).

## CacheKind Enum (`src/tree/node.rs`)

Add two variants. Preserve Default at `Unknown`.

| Variant | `label()` | `description()` | `url()` |
|---------|-----------|------------------|---------|
| `SwiftPm` | `"Swift Package Manager"` | `"Swift Package Manager — cached git clones, artifacts, and manifests"` | `"https://www.swift.org/package-manager/"` |
| `Xcode` | `"Xcode"` | `"Xcode build caches — DerivedData, iOS DeviceSupport, Simulator"` | `"https://developer.apple.com/xcode/"` |

## Config (`src/config.rs`)

Add to `Config::default()`:

```rust
#[cfg(target_os = "macos")]
{
    let swiftpm = home.join("Library/Caches/org.swift.swiftpm");
    if swiftpm.exists() {
        roots.push(swiftpm);
    }
    let derived_data = home.join("Library/Developer/Xcode/DerivedData");
    if derived_data.exists() {
        roots.push(derived_data);
    }
    let device_support = home.join("Library/Developer/Xcode/iOS DeviceSupport");
    if device_support.exists() {
        roots.push(device_support);
    }
    let coresim_caches = home.join("Library/Developer/CoreSimulator/Caches");
    if coresim_caches.exists() {
        roots.push(coresim_caches);
    }
}

// Linux SwiftPM path (macOS-only providers above; Linux users may still use SwiftPM)
let swiftpm_linux = home.join(".cache/org.swift.swiftpm");
if swiftpm_linux.exists() && !roots.iter().any(|r| r == &swiftpm_linux) {
    roots.push(swiftpm_linux);
}
```

No subprocess probing (no `probe_*_paths()`). All paths are static.

`default_for_test()` is unaffected (it returns `roots: vec![]` regardless).

A config test asserts the new roots appear when the directories exist; skips cleanly when they don't (CI without Xcode installed stays green).

## UI Impact

None. The tree view, detail panel, and deletion flow all work generically off `CacheKind`, `MetadataField`, and `SafetyLevel`. New providers slot in automatically.

## Testing Strategy

### Tier 1: Unit Tests (in-module, `#[cfg(test)]`)

Hardcore TDD: every function gets a failing test before implementation. Tests live in `src/providers/swiftpm.rs` and `src/providers/xcode.rs`. Use `tempfile::tempdir()` for fixtures involving `Info.plist`.

**SwiftPM tests (required):**
- `detect_swiftpm_library_caches_root` (macOS path).
- `detect_swiftpm_linux_cache_root`.
- `detect_swiftpm_repositories_subdir`.
- `detect_swiftpm_rejects_confusable_suffix` — path ending `org.swift.swiftpm-backup` must NOT detect as SwiftPm (L1).
- `semantic_name_strips_hash_suffix_from_repository`.
- `semantic_name_passes_through_artifacts_dir`.
- `semantic_name_returns_none_for_manifests_file`.
- `semantic_name_handles_non_ascii_package_name` (L2) — e.g. `café-a1b2c3d4`.
- `package_id_always_none`.
- `safety_repositories_is_caution`.
- `safety_artifacts_is_safe`.
- `safety_manifests_is_safe`.
- `safety_rejects_confusable_suffix` — `org.swift.swiftpm/repositories-old/foo` must NOT be classified as Caution-repositories.

**Xcode tests (required):**
- `detect_xcode_derived_data`.
- `detect_xcode_ios_device_support`.
- `detect_xcode_core_simulator_caches`.
- `detect_xcode_rejects_confusable_suffix` — `Xcode/DerivedData-backup/` must NOT detect (L1).
- `semantic_name_derived_data_from_info_plist` — fixture `Info.plist` with `WORKSPACE_PATH=/Users/j/dev/MyApp/MyApp.xcworkspace` → `"MyApp.xcworkspace (at /Users/j/dev/MyApp/MyApp.xcworkspace)"`.
- `semantic_name_derived_data_missing_plist_falls_back_to_dirname`.
- `semantic_name_derived_data_malformed_plist_falls_back_to_dirname` — truncated XML must not panic.
- `semantic_name_derived_data_non_ascii_workspace_path` (L2, L5) — `WORKSPACE_PATH` containing CJK characters renders correctly, no byte-boundary panic.
- `semantic_name_ios_device_support_uses_dirname`.
- `semantic_name_core_simulator_returns_none`.
- `package_id_always_none`.
- `safety_derived_data_is_caution`.
- `safety_ios_device_support_is_safe`.
- `safety_core_simulator_caches_is_safe`.
- `safety_rejects_confusable_suffix` — `DerivedData-backup/` NOT classified as Caution-DerivedData.

All tests must use `Config::default_for_test()` when a config is needed (L6 — no subprocess).

### Tier 2: Integration Tests (`tests/integration.rs`)

Extend existing integration test file (if the pattern fits) OR add a small file `tests/integration_swiftpm_xcode.rs`:

- Build synthetic fixtures in tempdir: fake `org.swift.swiftpm/` with all three subdirs populated; fake `DerivedData/MyApp-abc/Info.plist` with known `WORKSPACE_PATH`.
- Run `discover_packages()` and assert both providers' roots are walked.
- Assert semantic names render correctly through the full pipeline.
- Assert safety classification flows through to the detail panel's icon selection.

### Tier 3: E2E Tests

**Explicitly exempt.** Rationale documented in-module:

- Neither provider participates in OSV or the version-check registry. The `feedback_e2e_full_pipeline` rubric ("install real tools, download outdated+vulnerable packages, verify OSV + version-check fire") describes a pipeline that does not apply here.
- The disk-hygiene behavior is fully covered by Tier 1 (functional correctness on synthetic fixtures) + Tier 2 (full pipeline on synthetic fixtures).
- Writing a synthetic-only E2E file would duplicate Tier 2 coverage under a different feature flag without adding signal.

Each provider module starts with a short comment header noting this exemption and why, so a future reviewer comparing against `adding-a-provider.md` §2's E2E checkbox knows the decision was deliberate.

## Docs Updates

- `README.md` — add two rows to the Supported Caches table; bump provider count; add SwiftPM + Xcode to the "Why" rationale bullet if relevant.
- `CHANGELOG.md` — `[Unreleased]` / `### Added` entries for both providers. Explicitly note: (a) SwiftPM's `c` upgrade key is a no-op by design; (b) neither provider runs OSV or version-check.
- `docs/adding-a-provider.md` — append "Lessons learned" entry after the PR merges (§5): one line each for SwiftPM (package_id-skip rationale) and Xcode (Info.plist parsing as non-OSV signal source).
- `TODO.md` — tick issue #11 and #17.

## Files Changed

| File | Change |
|------|--------|
| `src/tree/node.rs` | Add `SwiftPm`, `Xcode` to `CacheKind` enum with label/description/url |
| `src/providers/mod.rs` | Add `pub mod swiftpm; pub mod xcode;`; add match arms in `detect`, `semantic_name`, `metadata`, `safety` |
| `src/providers/swiftpm.rs` | **New** — SwiftPM provider |
| `src/providers/xcode.rs` | **New** — Xcode/DerivedData provider |
| `src/config.rs` | Add default roots for both providers (macOS + Linux SwiftPM), with `exists()` guards |
| `tests/integration_swiftpm_xcode.rs` | **New** (or extend existing) — synthetic fixture pipeline tests |
| `README.md` | Update Supported Caches table + count |
| `CHANGELOG.md` | `[Unreleased]` entries noting OSV/version-check exemption |
| `docs/adding-a-provider.md` | Append Lessons learned lines after merge |
| `TODO.md` | Tick #11 and #17 |

## Out of Scope

- SwiftPM `package_id` / OSV / version-check integration (future PR once OSV `SwiftURL` coverage justifies it).
- Parsing Xcode's `Info.plist` binary variant (we observe only XML in practice; will revisit if a user reports binary-plist output).
- Distinguishing active vs stale DerivedData projects (would require correlating with disk-present workspace files — out of scope for disk hygiene).
- Linux Xcode provider support (Xcode is Apple-only; we scope the `#[cfg(target_os = "macos")]` block accordingly, but SwiftPM's Linux path IS included).
- Detecting `~/Library/Developer/CoreSimulator/Devices/` (this contains active simulator state and user data — deleting breaks users' simulator setups; explicitly excluded as unsafe).

## Risks

- **Info.plist format drift:** If Apple ever switches DerivedData's `Info.plist` to binary format, XML string-search parsing returns `None`, and we fall back to the directory name. No crash, just degraded semantic names. Acceptable.
- **SwiftPM layout change:** The `repositories/`, `artifacts/`, `manifests/` triad is stable as of Swift 5.9, but SwiftPM is evolving. Unknown future subdirs get `Caution` safety by default — conservative fail-safe.
- **Hash suffix false positive:** A package legitimately ending in `-<8hex>` would have its hash stripped. Real-world hit rate is near zero; acceptable.
