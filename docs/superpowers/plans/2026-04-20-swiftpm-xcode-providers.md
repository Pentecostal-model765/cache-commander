# SwiftPM + Xcode Providers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add SwiftPM and Xcode DerivedData cache providers to ccmd (issues #11, #17).

**Architecture:** Two independent provider modules (`src/providers/swiftpm.rs`, `src/providers/xcode.rs`) wired through the existing dispatch (`detect`/`semantic_name`/`metadata`/`safety`) in `src/providers/mod.rs`. Neither provider returns a `PackageId`, so OSV/version-check/upgrade-command machinery is untouched. Two new `CacheKind` variants. Static config roots guarded by `exists()` checks — no subprocess probing.

**Tech Stack:** Rust 2024 edition, existing deps (no new crates). `tempfile` for fixture-driven tests. All tests strict TDD: red → green → refactor, one assertion per intent.

**Spec:** `docs/superpowers/specs/2026-04-20-swiftpm-xcode-providers-design.md`

---

## File Structure

| File | Role |
|------|------|
| `src/tree/node.rs` | Modify: add `SwiftPm` + `Xcode` to `CacheKind` enum with label/description/url |
| `src/providers/mod.rs` | Modify: add module decls + dispatch arms (`detect`, `semantic_name`, `metadata`, `safety`); `package_id` + `upgrade_command` fall through |
| `src/providers/swiftpm.rs` | Create: SwiftPM provider (semantic_name, metadata; no package_id) |
| `src/providers/xcode.rs` | Create: Xcode provider (semantic_name, metadata; Info.plist WORKSPACE_PATH extraction; no package_id) |
| `src/config.rs` | Modify: add default roots for SwiftPM (macOS + Linux) and Xcode (macOS only) |
| `tests/integration_swiftpm_xcode.rs` | Create: synthetic-fixture pipeline tests (discover + semantic name + safety) |
| `README.md` | Modify: bump supported-caches count; add table rows |
| `CHANGELOG.md` | Modify: `[Unreleased]` Added bullets (note OSV/version-check exemption) |
| `TODO.md` | Modify: tick issues #11 and #17 |
| `docs/adding-a-provider.md` | Modify: append lessons-learned line after merge |

---

## Task 1: Branch + CacheKind enum additions

**Files:**
- Modify: `src/tree/node.rs` (CacheKind enum + label/description/url methods)

- [ ] **Step 1: Create feature branch**

```bash
git checkout -b feat/swiftpm-xcode-providers
```

- [ ] **Step 2: Write failing tests for new CacheKind variants**

Append to `src/tree/node.rs` tests module:

```rust
#[test]
fn cachekind_swiftpm_label_description_url() {
    assert_eq!(CacheKind::SwiftPm.label(), "Swift Package Manager");
    assert!(!CacheKind::SwiftPm.description().is_empty());
    assert_eq!(
        CacheKind::SwiftPm.url(),
        "https://www.swift.org/package-manager/"
    );
}

#[test]
fn cachekind_xcode_label_description_url() {
    assert_eq!(CacheKind::Xcode.label(), "Xcode");
    assert!(!CacheKind::Xcode.description().is_empty());
    assert_eq!(CacheKind::Xcode.url(), "https://developer.apple.com/xcode/");
}
```

- [ ] **Step 3: Run tests, confirm compile error (variants don't exist)**

```bash
cargo test --lib tree::node::tests::cachekind_swiftpm 2>&1 | tail -20
```

Expected: `error[E0599]: no variant or associated item named 'SwiftPm' found for enum 'CacheKind'`

- [ ] **Step 4: Add variants and method arms**

Edit `src/tree/node.rs`:
- Add `SwiftPm,` and `Xcode,` to the `CacheKind` enum (before `Unknown`).
- Add corresponding arms in `label()`, `description()`, and `url()` matching the spec table.

```rust
// label()
Self::SwiftPm => "Swift Package Manager",
Self::Xcode => "Xcode",
// description()
Self::SwiftPm => "Swift Package Manager — cached git clones, artifacts, and manifests",
Self::Xcode => "Xcode build caches — DerivedData, iOS DeviceSupport, Simulator",
// url()
Self::SwiftPm => "https://www.swift.org/package-manager/",
Self::Xcode => "https://developer.apple.com/xcode/",
```

- [ ] **Step 5: Run tests, confirm pass**

```bash
cargo test --lib tree::node::tests::cachekind_swiftpm cargo test --lib tree::node::tests::cachekind_xcode
```

Expected: 2 passed.

- [ ] **Step 6: Run full test suite to confirm no regressions**

```bash
cargo test --lib
```

Expected: all tests pass (the dispatcher `_ => None` paths cover the new variants for now).

- [ ] **Step 7: Commit**

```bash
git add src/tree/node.rs
git commit -m "feat(providers): add SwiftPm and Xcode variants to CacheKind"
```

---

## Task 2: SwiftPM provider — module scaffold + detect()

**Files:**
- Create: `src/providers/swiftpm.rs`
- Modify: `src/providers/mod.rs` (add `pub mod swiftpm;` + `detect` arm)

- [ ] **Step 1: Write failing detect tests**

Append to `src/providers/mod.rs` tests:

```rust
#[test]
fn detect_swiftpm_library_caches_root() {
    assert_eq!(
        detect(&PathBuf::from(
            "/Users/j/Library/Caches/org.swift.swiftpm"
        )),
        CacheKind::SwiftPm
    );
}

#[test]
fn detect_swiftpm_linux_cache_root() {
    assert_eq!(
        detect(&PathBuf::from("/home/u/.cache/org.swift.swiftpm")),
        CacheKind::SwiftPm
    );
}

#[test]
fn detect_swiftpm_repositories_subdir() {
    assert_eq!(
        detect(&PathBuf::from(
            "/Users/j/Library/Caches/org.swift.swiftpm/repositories/swift-collections-abc1234"
        )),
        CacheKind::SwiftPm
    );
}

#[test]
fn detect_swiftpm_rejects_confusable_suffix() {
    // L1: substring match would accept this; component match must reject.
    assert_ne!(
        detect(&PathBuf::from(
            "/Users/j/Library/Caches/org.swift.swiftpm-backup"
        )),
        CacheKind::SwiftPm
    );
}
```

- [ ] **Step 2: Run tests, confirm fail**

```bash
cargo test --lib providers::tests::detect_swiftpm
```

Expected: all 4 fail (return `Unknown`).

- [ ] **Step 3: Create minimal `swiftpm.rs` + wire detect arm**

`src/providers/swiftpm.rs`:

```rust
// SwiftPM provider.
//
// No `package_id` / `upgrade_command` arms — SwiftPM's package identity
// requires git-ref parsing that is too brittle for v1, and there's no
// public registry for version lookups. OSV `SwiftURL` coverage is
// sparse. Tier-3 E2E tests are intentionally exempt (see design spec).

use super::MetadataField;
use std::path::Path;

pub fn semantic_name(_path: &Path) -> Option<String> {
    None
}

pub fn metadata(_path: &Path) -> Vec<MetadataField> {
    Vec::new()
}
```

In `src/providers/mod.rs`:
- Add `pub mod swiftpm;` (alphabetical, after `pre_commit`/`prisma`).
- Add to `detect()` direct-name arm: `"org.swift.swiftpm" => return CacheKind::SwiftPm,`
- Add to ancestor-walk arm the same match.
- Add `CacheKind::SwiftPm => swiftpm::semantic_name(path),` to `semantic_name` dispatch.
- Add `CacheKind::SwiftPm => swiftpm::metadata(path),` to `metadata` dispatch.

- [ ] **Step 4: Run tests, confirm pass**

```bash
cargo test --lib providers::tests::detect_swiftpm
```

Expected: all 4 pass.

- [ ] **Step 5: Commit**

```bash
git add src/providers/swiftpm.rs src/providers/mod.rs
git commit -m "feat(providers): scaffold SwiftPM provider with detect()"
```

---

## Task 3: SwiftPM `semantic_name`

**Files:**
- Modify: `src/providers/swiftpm.rs`

- [ ] **Step 1: Write failing semantic_name tests**

Append to `src/providers/swiftpm.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn semantic_name_strips_hash_suffix_from_repository() {
        let path = PathBuf::from(
            "/Users/j/Library/Caches/org.swift.swiftpm/repositories/swift-collections-abc1234",
        );
        assert_eq!(semantic_name(&path), Some("swift-collections".into()));
    }

    #[test]
    fn semantic_name_strips_long_hash_suffix() {
        let path = PathBuf::from(
            "/Users/j/Library/Caches/org.swift.swiftpm/repositories/swift-nio-0123456789abcdef",
        );
        assert_eq!(semantic_name(&path), Some("swift-nio".into()));
    }

    #[test]
    fn semantic_name_passes_through_artifacts_dir() {
        let path = PathBuf::from(
            "/Users/j/Library/Caches/org.swift.swiftpm/artifacts/MyBinaryDep",
        );
        assert_eq!(semantic_name(&path), Some("MyBinaryDep".into()));
    }

    #[test]
    fn semantic_name_returns_none_for_manifests_file() {
        // manifest filenames are internal hashes; no meaningful display.
        let path = PathBuf::from(
            "/Users/j/Library/Caches/org.swift.swiftpm/manifests/deadbeef1234",
        );
        assert_eq!(semantic_name(&path), None);
    }

    #[test]
    fn semantic_name_returns_none_for_known_subdir_roots() {
        // The three subdir names themselves should not be renamed.
        for subdir in ["repositories", "artifacts", "manifests"] {
            let path = PathBuf::from(format!(
                "/Users/j/Library/Caches/org.swift.swiftpm/{subdir}"
            ));
            assert_eq!(semantic_name(&path), None, "{subdir}");
        }
    }

    #[test]
    fn semantic_name_handles_non_ascii_package_name() {
        // L2: no byte-boundary panic.
        let path = PathBuf::from(
            "/Users/j/Library/Caches/org.swift.swiftpm/repositories/café-abc1234",
        );
        assert_eq!(semantic_name(&path), Some("café".into()));
    }

    #[test]
    fn semantic_name_returns_none_for_name_without_hash_suffix() {
        // Short/missing hex suffix doesn't match our rule.
        let path = PathBuf::from(
            "/Users/j/Library/Caches/org.swift.swiftpm/repositories/plain-name",
        );
        // No trailing -<hex7+>, so no transformation; fall through to pass-through OR None.
        // Our rule: repositories subdir requires hash suffix; without it, we still
        // fall back to the dir name (user-friendly).
        assert_eq!(semantic_name(&path), Some("plain-name".into()));
    }
}
```

- [ ] **Step 2: Run tests, confirm fail**

```bash
cargo test --lib providers::swiftpm::tests
```

Expected: all 7 fail (current impl returns None for everything).

- [ ] **Step 3: Implement semantic_name**

Replace the stub in `src/providers/swiftpm.rs`:

```rust
pub fn semantic_name(path: &Path) -> Option<String> {
    // Determine position relative to `org.swift.swiftpm/<subdir>/<item>`.
    // We need the immediate parent subdir to decide how to interpret the name.
    let name = path.file_name()?.to_string_lossy().to_string();

    // Skip the three known subdir roots themselves.
    if matches!(name.as_str(), "repositories" | "artifacts" | "manifests") {
        return None;
    }

    let parent_name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    match parent_name.as_str() {
        "repositories" => Some(strip_hash_suffix(&name)),
        "artifacts" => Some(name),
        "manifests" => None, // internal hash filenames
        _ => None,
    }
}

/// Strip a trailing `-<hex>` where hex is 7 or more lowercase-hex chars.
/// Returns the input unchanged if no such suffix exists.
fn strip_hash_suffix(s: &str) -> String {
    let Some(dash_idx) = s.rfind('-') else {
        return s.to_string();
    };
    let suffix = &s[dash_idx + 1..];
    if suffix.len() >= 7 && suffix.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        s[..dash_idx].to_string()
    } else {
        s.to_string()
    }
}
```

- [ ] **Step 4: Run tests, confirm pass**

```bash
cargo test --lib providers::swiftpm::tests
```

Expected: 7 passed.

- [ ] **Step 5: Run full suite**

```bash
cargo test --lib
```

Expected: no regressions.

- [ ] **Step 6: Commit**

```bash
git add src/providers/swiftpm.rs
git commit -m "feat(swiftpm): semantic_name with hash-suffix stripping"
```

---

## Task 4: SwiftPM `metadata`

**Files:**
- Modify: `src/providers/swiftpm.rs`

- [ ] **Step 1: Write failing metadata tests**

Add to the tests module in `src/providers/swiftpm.rs`:

```rust
#[test]
fn metadata_repositories_root_reports_contents() {
    let path = PathBuf::from(
        "/Users/j/Library/Caches/org.swift.swiftpm/repositories",
    );
    let fields = metadata(&path);
    assert!(fields.iter().any(|f| f.label == "Contents" && f.value.contains("Git clones")));
}

#[test]
fn metadata_artifacts_root_reports_contents() {
    let path = PathBuf::from(
        "/Users/j/Library/Caches/org.swift.swiftpm/artifacts",
    );
    let fields = metadata(&path);
    assert!(fields.iter().any(|f| f.label == "Contents" && f.value.contains("Binary artifacts")));
}

#[test]
fn metadata_manifests_root_reports_contents() {
    let path = PathBuf::from(
        "/Users/j/Library/Caches/org.swift.swiftpm/manifests",
    );
    let fields = metadata(&path);
    assert!(fields.iter().any(|f| f.label == "Contents" && f.value.contains("Package.swift")));
}

#[test]
fn metadata_leaf_file_returns_empty() {
    let path = PathBuf::from(
        "/Users/j/Library/Caches/org.swift.swiftpm/repositories/swift-collections-abc1234",
    );
    assert!(metadata(&path).is_empty());
}
```

- [ ] **Step 2: Run tests, confirm fail**

Expected: 3 fail (the `_leaf_` test passes since the stub already returns empty).

- [ ] **Step 3: Implement metadata**

Replace the stub:

```rust
pub fn metadata(path: &Path) -> Vec<MetadataField> {
    let mut fields = Vec::new();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent_is_swiftpm = path
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == "org.swift.swiftpm");
    if parent_is_swiftpm {
        let value = match name.as_str() {
            "repositories" => Some("Git clones of package sources (re-cloneable)"),
            "artifacts" => Some("Binary artifacts (re-downloadable)"),
            "manifests" => Some("Cached Package.swift resolutions"),
            _ => None,
        };
        if let Some(v) = value {
            fields.push(MetadataField {
                label: "Contents".to_string(),
                value: v.to_string(),
            });
        }
    }
    fields
}
```

- [ ] **Step 4: Run tests, confirm pass**

```bash
cargo test --lib providers::swiftpm::tests
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/providers/swiftpm.rs
git commit -m "feat(swiftpm): metadata for root subdirs"
```

---

## Task 5: SwiftPM `safety` (dispatch in mod.rs)

**Files:**
- Modify: `src/providers/mod.rs` (safety arm)

- [ ] **Step 1: Write failing safety tests**

Append to `src/providers/mod.rs` tests:

```rust
#[test]
fn safety_swiftpm_repositories_is_caution() {
    let path = PathBuf::from(
        "/Users/j/Library/Caches/org.swift.swiftpm/repositories/swift-collections-abc1234",
    );
    assert_eq!(safety(CacheKind::SwiftPm, &path), SafetyLevel::Caution);
}

#[test]
fn safety_swiftpm_artifacts_is_safe() {
    let path = PathBuf::from(
        "/Users/j/Library/Caches/org.swift.swiftpm/artifacts/MyBinaryDep",
    );
    assert_eq!(safety(CacheKind::SwiftPm, &path), SafetyLevel::Safe);
}

#[test]
fn safety_swiftpm_manifests_is_safe() {
    let path = PathBuf::from(
        "/Users/j/Library/Caches/org.swift.swiftpm/manifests/deadbeef",
    );
    assert_eq!(safety(CacheKind::SwiftPm, &path), SafetyLevel::Safe);
}

#[test]
fn safety_swiftpm_unknown_subdir_is_caution() {
    let path = PathBuf::from(
        "/Users/j/Library/Caches/org.swift.swiftpm/futuredir/item",
    );
    assert_eq!(safety(CacheKind::SwiftPm, &path), SafetyLevel::Caution);
}

#[test]
fn safety_swiftpm_rejects_confusable_suffix() {
    // L1: `repositories-old` must NOT classify as repositories.
    let path = PathBuf::from(
        "/Users/j/Library/Caches/org.swift.swiftpm/repositories-old/foo",
    );
    // This should be Caution (unknown subdir), not specifically
    // "repositories Caution" — but both are Caution. Verify the *reason*
    // by also checking a Safe path with confusable parent.
    assert_eq!(safety(CacheKind::SwiftPm, &path), SafetyLevel::Caution);
}
```

- [ ] **Step 2: Run tests, confirm fail**

Expected: `safety_swiftpm_artifacts_is_safe` and `_manifests_is_safe` fail (currently falls through to `Safe` default — wait, actually the default IS Safe. So those two would pass. The two Caution expectations would fail).

- [ ] **Step 3: Implement safety arm**

In `src/providers/mod.rs`, add to the `safety()` function match before the `_ => SafetyLevel::Safe` fallback:

```rust
CacheKind::SwiftPm => {
    // Walk path components looking for the known subdir immediately
    // after `org.swift.swiftpm`. Component-based to avoid L1 substring
    // false positives.
    let comps: Vec<&std::ffi::OsStr> = path.components().map(|c| c.as_os_str()).collect();
    let mut classified = None;
    for w in comps.windows(2) {
        if w[0] == "org.swift.swiftpm" {
            classified = match w[1].to_string_lossy().as_ref() {
                "repositories" => Some(SafetyLevel::Caution),
                "artifacts" | "manifests" => Some(SafetyLevel::Safe),
                _ => Some(SafetyLevel::Caution), // unknown future subdirs
            };
            break;
        }
    }
    // Exact-root case (path is `.../org.swift.swiftpm` itself): Caution.
    classified.unwrap_or(SafetyLevel::Caution)
}
```

- [ ] **Step 4: Run tests, confirm pass**

```bash
cargo test --lib providers::tests::safety_swiftpm
```

Expected: all 5 pass.

- [ ] **Step 5: Commit**

```bash
git add src/providers/mod.rs
git commit -m "feat(swiftpm): safety classification (repositories=Caution, others=Safe)"
```

---

## Task 6: Xcode provider — module scaffold + detect()

**Files:**
- Create: `src/providers/xcode.rs`
- Modify: `src/providers/mod.rs`

- [ ] **Step 1: Write failing detect tests**

Append to `src/providers/mod.rs` tests:

```rust
#[test]
fn detect_xcode_derived_data() {
    assert_eq!(
        detect(&PathBuf::from(
            "/Users/j/Library/Developer/Xcode/DerivedData"
        )),
        CacheKind::Xcode
    );
}

#[test]
fn detect_xcode_derived_data_project_subdir() {
    assert_eq!(
        detect(&PathBuf::from(
            "/Users/j/Library/Developer/Xcode/DerivedData/MyApp-abc123def456"
        )),
        CacheKind::Xcode
    );
}

#[test]
fn detect_xcode_ios_device_support() {
    assert_eq!(
        detect(&PathBuf::from(
            "/Users/j/Library/Developer/Xcode/iOS DeviceSupport/17.4 (21E213)"
        )),
        CacheKind::Xcode
    );
}

#[test]
fn detect_xcode_core_simulator_caches() {
    assert_eq!(
        detect(&PathBuf::from(
            "/Users/j/Library/Developer/CoreSimulator/Caches/something"
        )),
        CacheKind::Xcode
    );
}

#[test]
fn detect_xcode_rejects_confusable_suffix() {
    // L1: Xcode/DerivedData-backup must not match.
    assert_ne!(
        detect(&PathBuf::from(
            "/Users/j/Library/Developer/Xcode/DerivedData-backup"
        )),
        CacheKind::Xcode
    );
}

#[test]
fn detect_xcode_rejects_unrelated_derived_data() {
    // A DerivedData directory not under Xcode must not match.
    assert_ne!(
        detect(&PathBuf::from("/random/path/DerivedData")),
        CacheKind::Xcode
    );
}
```

- [ ] **Step 2: Run tests, confirm fail**

Expected: all 4 positive cases fail (return Unknown). Negative cases pass trivially.

- [ ] **Step 3: Create `xcode.rs` + wire detect**

`src/providers/xcode.rs`:

```rust
// Xcode provider (DerivedData, iOS DeviceSupport, CoreSimulator Caches).
//
// No package identity / OSV / version-check / upgrade-command: these are
// build artifacts, not packages. Tier-3 E2E tests intentionally exempt
// (see design spec).

use super::MetadataField;
use std::path::Path;

pub fn semantic_name(_path: &Path) -> Option<String> {
    None
}

pub fn metadata(_path: &Path) -> Vec<MetadataField> {
    Vec::new()
}
```

In `src/providers/mod.rs`:
- Add `pub mod xcode;` (alphabetical).
- Add `CacheKind::Xcode => xcode::semantic_name(path),` to `semantic_name` dispatch.
- Add `CacheKind::Xcode => xcode::metadata(path),` to `metadata` dispatch.
- In `detect()`, add this block BEFORE the final `Unknown` return:

```rust
if has_adjacent_components(path, "Xcode", "DerivedData")
    || has_adjacent_components(path, "Xcode", "iOS DeviceSupport")
    || has_adjacent_components(path, "CoreSimulator", "Caches")
{
    return CacheKind::Xcode;
}
```

- [ ] **Step 4: Run tests, confirm pass**

```bash
cargo test --lib providers::tests::detect_xcode
```

Expected: 6 pass.

- [ ] **Step 5: Commit**

```bash
git add src/providers/xcode.rs src/providers/mod.rs
git commit -m "feat(providers): scaffold Xcode provider with detect()"
```

---

## Task 7: Xcode `semantic_name` — DerivedData Info.plist extraction

**Files:**
- Modify: `src/providers/xcode.rs`

- [ ] **Step 1: Write failing semantic_name tests**

Append to `src/providers/xcode.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_derived_data_dir(tmp: &tempfile::TempDir, workspace_path: Option<&str>) -> PathBuf {
        let root = tmp.path().join("Library/Developer/Xcode/DerivedData/MyApp-abc123def");
        std::fs::create_dir_all(&root).unwrap();
        if let Some(wp) = workspace_path {
            let plist = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>WORKSPACE_PATH</key>
    <string>{wp}</string>
</dict>
</plist>"#
            );
            std::fs::write(root.join("Info.plist"), plist).unwrap();
        }
        root
    }

    #[test]
    fn semantic_name_derived_data_from_info_plist() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = make_derived_data_dir(&tmp, Some("/Users/j/dev/MyApp/MyApp.xcworkspace"));
        assert_eq!(
            semantic_name(&dir),
            Some("MyApp.xcworkspace (at /Users/j/dev/MyApp/MyApp.xcworkspace)".into())
        );
    }

    #[test]
    fn semantic_name_derived_data_missing_plist_falls_back_to_dirname() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = make_derived_data_dir(&tmp, None);
        assert_eq!(semantic_name(&dir), Some("MyApp-abc123def".into()));
    }

    #[test]
    fn semantic_name_derived_data_malformed_plist_falls_back_to_dirname() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Library/Developer/Xcode/DerivedData/Broken-xyz");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Info.plist"), "<plist><incomplete").unwrap();
        assert_eq!(semantic_name(&root), Some("Broken-xyz".into()));
    }

    #[test]
    fn semantic_name_derived_data_non_ascii_workspace_path() {
        // L2 / L5: no panic on multi-byte chars.
        let tmp = tempfile::tempdir().unwrap();
        let dir = make_derived_data_dir(&tmp, Some("/Users/j/日本語/App.xcworkspace"));
        assert_eq!(
            semantic_name(&dir),
            Some("App.xcworkspace (at /Users/j/日本語/App.xcworkspace)".into())
        );
    }

    #[test]
    fn semantic_name_ios_device_support_uses_dirname() {
        let path = PathBuf::from(
            "/Users/j/Library/Developer/Xcode/iOS DeviceSupport/17.4 (21E213)",
        );
        assert_eq!(semantic_name(&path), Some("17.4 (21E213)".into()));
    }

    #[test]
    fn semantic_name_core_simulator_returns_none() {
        let path = PathBuf::from(
            "/Users/j/Library/Developer/CoreSimulator/Caches/com.apple.SimulatorTrampoline",
        );
        assert_eq!(semantic_name(&path), None);
    }

    #[test]
    fn semantic_name_known_roots_return_none() {
        // The three root directories themselves should render literally.
        for p in [
            "/Users/j/Library/Developer/Xcode/DerivedData",
            "/Users/j/Library/Developer/Xcode/iOS DeviceSupport",
            "/Users/j/Library/Developer/CoreSimulator/Caches",
        ] {
            assert_eq!(semantic_name(&PathBuf::from(p)), None, "{p}");
        }
    }
}
```

- [ ] **Step 2: Run tests, confirm fail**

Expected: the two `_returns_none` tests pass (stub returns None); the 5 positive cases fail.

- [ ] **Step 3: Implement semantic_name**

Replace the stub in `src/providers/xcode.rs`:

```rust
pub fn semantic_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy().to_string();

    // Known root subdirs — render literally.
    if matches!(name.as_str(), "DerivedData" | "iOS DeviceSupport" | "Caches") {
        return None;
    }

    // DerivedData project dir: try Info.plist WORKSPACE_PATH, fall back to name.
    let parent_name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    if parent_name == "DerivedData" {
        if let Some(workspace_path) = read_workspace_path(path) {
            let basename = workspace_path
                .rsplit('/')
                .next()
                .unwrap_or(&workspace_path)
                .to_string();
            return Some(format!("{basename} (at {workspace_path})"));
        }
        return Some(name);
    }

    if parent_name == "iOS DeviceSupport" {
        return Some(name);
    }

    // CoreSimulator/Caches entries: opaque, no semantic name.
    None
}

/// Read WORKSPACE_PATH from Info.plist (XML variant).
/// Returns None on any failure (missing file, malformed XML, missing key).
fn read_workspace_path(dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(dir.join("Info.plist")).ok()?;
    extract_plist_string(&content, "WORKSPACE_PATH")
}

/// Extract a string value by key from an XML plist body. Simple string
/// scanner — avoids a plist-crate dependency. Uses char-boundary-safe
/// string APIs (L2, L5).
fn extract_plist_string(xml: &str, key: &str) -> Option<String> {
    let key_tag = format!("<key>{key}</key>");
    let key_pos = xml.find(&key_tag)?;
    let after_key = &xml[key_pos + key_tag.len()..];
    let open = "<string>";
    let open_pos = after_key.find(open)?;
    let value_start = open_pos + open.len();
    let close_pos = after_key[value_start..].find("</string>")?;
    let value = &after_key[value_start..value_start + close_pos];
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
```

- [ ] **Step 4: Run tests, confirm pass**

```bash
cargo test --lib providers::xcode::tests
```

Expected: 7 pass.

- [ ] **Step 5: Commit**

```bash
git add src/providers/xcode.rs
git commit -m "feat(xcode): semantic_name with Info.plist WORKSPACE_PATH extraction"
```

---

## Task 8: Xcode `metadata`

**Files:**
- Modify: `src/providers/xcode.rs`

- [ ] **Step 1: Write failing metadata tests**

Add to the `xcode::tests` module:

```rust
#[test]
fn metadata_derived_data_root_reports_contents() {
    let path = PathBuf::from("/Users/j/Library/Developer/Xcode/DerivedData");
    let fields = metadata(&path);
    assert!(fields.iter().any(|f| f.label == "Contents" && f.value.contains("build products")));
}

#[test]
fn metadata_ios_device_support_root_reports_contents() {
    let path = PathBuf::from("/Users/j/Library/Developer/Xcode/iOS DeviceSupport");
    let fields = metadata(&path);
    assert!(fields.iter().any(|f| f.label == "Contents" && f.value.contains("Symbol files")));
}

#[test]
fn metadata_core_simulator_caches_root_reports_contents() {
    let path = PathBuf::from("/Users/j/Library/Developer/CoreSimulator/Caches");
    let fields = metadata(&path);
    assert!(fields.iter().any(|f| f.label == "Contents" && f.value.contains("Simulator")));
}

#[test]
fn metadata_derived_data_project_dir_emits_workspace_path_when_available() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = make_derived_data_dir(&tmp, Some("/Users/j/dev/MyApp/MyApp.xcworkspace"));
    let fields = metadata(&dir);
    assert!(
        fields.iter().any(|f| f.label == "Workspace"
            && f.value == "/Users/j/dev/MyApp/MyApp.xcworkspace"),
        "expected Workspace field, got {fields:?}"
    );
}

#[test]
fn metadata_derived_data_project_dir_without_plist_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = make_derived_data_dir(&tmp, None);
    assert!(metadata(&dir).is_empty());
}
```

- [ ] **Step 2: Run tests, confirm fail**

Expected: 4 fail (the `_without_plist_is_empty` passes since stub returns empty).

- [ ] **Step 3: Implement metadata**

Replace the stub:

```rust
pub fn metadata(path: &Path) -> Vec<MetadataField> {
    let mut fields = Vec::new();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent_name = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // Root-level labels.
    match name.as_str() {
        "DerivedData" => {
            fields.push(MetadataField {
                label: "Contents".into(),
                value: "Xcode build products and indexes (rebuildable, 5–30 min cost)".into(),
            });
            return fields;
        }
        "iOS DeviceSupport" => {
            fields.push(MetadataField {
                label: "Contents".into(),
                value: "Symbol files for connected iOS devices (re-downloadable on device reconnect)".into(),
            });
            return fields;
        }
        "Caches" if parent_name == "CoreSimulator" => {
            fields.push(MetadataField {
                label: "Contents".into(),
                value: "iOS Simulator caches (safe to clear)".into(),
            });
            return fields;
        }
        _ => {}
    }

    // DerivedData project dir: surface WORKSPACE_PATH if present.
    if parent_name == "DerivedData"
        && let Some(wp) = read_workspace_path(path)
    {
        fields.push(MetadataField {
            label: "Workspace".into(),
            value: wp,
        });
    }

    fields
}
```

- [ ] **Step 4: Run tests, confirm pass**

```bash
cargo test --lib providers::xcode::tests
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/providers/xcode.rs
git commit -m "feat(xcode): metadata for roots and DerivedData project dirs"
```

---

## Task 9: Xcode `safety` (dispatch in mod.rs)

**Files:**
- Modify: `src/providers/mod.rs`

- [ ] **Step 1: Write failing safety tests**

Append to `src/providers/mod.rs` tests:

```rust
#[test]
fn safety_xcode_derived_data_is_caution() {
    let path = PathBuf::from(
        "/Users/j/Library/Developer/Xcode/DerivedData/MyApp-abc",
    );
    assert_eq!(safety(CacheKind::Xcode, &path), SafetyLevel::Caution);
}

#[test]
fn safety_xcode_ios_device_support_is_safe() {
    let path = PathBuf::from(
        "/Users/j/Library/Developer/Xcode/iOS DeviceSupport/17.4 (21E213)",
    );
    assert_eq!(safety(CacheKind::Xcode, &path), SafetyLevel::Safe);
}

#[test]
fn safety_xcode_core_simulator_caches_is_safe() {
    let path = PathBuf::from(
        "/Users/j/Library/Developer/CoreSimulator/Caches/something",
    );
    assert_eq!(safety(CacheKind::Xcode, &path), SafetyLevel::Safe);
}

#[test]
fn safety_xcode_rejects_confusable_suffix_derived_data() {
    // L1: DerivedData-backup must not be classified as Caution-DerivedData.
    // Under CacheKind::Xcode, any path not matching a known subtree
    // falls through to Safe default.
    let path = PathBuf::from(
        "/Users/j/Library/Developer/Xcode/DerivedData-backup/junk",
    );
    assert_eq!(safety(CacheKind::Xcode, &path), SafetyLevel::Safe);
}
```

- [ ] **Step 2: Run tests, confirm fail**

Expected: `_derived_data_is_caution` fails (default is Safe). Others pass trivially.

- [ ] **Step 3: Add Xcode safety arm**

In `src/providers/mod.rs`, add to `safety()` match before the `_` fallback:

```rust
CacheKind::Xcode => {
    // Only DerivedData triggers Caution (rebuild cost). iOS
    // DeviceSupport and CoreSimulator caches are Safe. Component-based
    // matching avoids L1 substring leaks.
    if has_adjacent_components(path, "Xcode", "DerivedData") {
        SafetyLevel::Caution
    } else {
        SafetyLevel::Safe
    }
}
```

- [ ] **Step 4: Run tests, confirm pass**

```bash
cargo test --lib providers::tests::safety_xcode
```

Expected: 4 pass.

- [ ] **Step 5: Commit**

```bash
git add src/providers/mod.rs
git commit -m "feat(xcode): safety classification (DerivedData=Caution, others=Safe)"
```

---

## Task 10: Config — default roots for SwiftPM and Xcode

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write failing config test**

Append to `src/config.rs` tests module:

```rust
#[test]
#[cfg(target_os = "macos")]
fn default_config_includes_swiftpm_on_macos_when_exists() {
    let swiftpm = dirs_home().join("Library/Caches/org.swift.swiftpm");
    if !swiftpm.exists() {
        return; // graceful skip on hosts without SwiftPM
    }
    let config = Config::default();
    assert!(
        config.roots.iter().any(|r| r == &swiftpm),
        "expected SwiftPM root in config, got {:?}",
        config.roots
    );
}

#[test]
#[cfg(target_os = "macos")]
fn default_config_includes_derived_data_when_exists() {
    let dd = dirs_home().join("Library/Developer/Xcode/DerivedData");
    if !dd.exists() {
        return;
    }
    let config = Config::default();
    assert!(config.roots.iter().any(|r| r == &dd));
}

#[test]
fn default_for_test_is_empty_roots() {
    // Regression guard: adding new roots must not leak into test config.
    assert!(Config::default_for_test().roots.is_empty());
}
```

- [ ] **Step 2: Run tests, confirm they fail (if Xcode is installed locally) or skip**

```bash
cargo test --lib config::tests::default_config_includes_swiftpm
cargo test --lib config::tests::default_config_includes_derived_data
cargo test --lib config::tests::default_for_test_is_empty_roots
```

Expected: the two macOS path tests either fail (if dirs exist) or skip cleanly. The `default_for_test` test passes.

- [ ] **Step 3: Add default roots**

In `src/config.rs`, inside `impl Default for Config`, add after the existing `gradle_caches` block and before the `probe_yarn_paths()` block:

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

// Linux SwiftPM (macOS path handled above).
#[cfg(not(target_os = "macos"))]
{
    let swiftpm_linux = home.join(".cache/org.swift.swiftpm");
    if swiftpm_linux.exists() && !roots.contains(&swiftpm_linux) {
        roots.push(swiftpm_linux);
    }
}
```

- [ ] **Step 4: Run tests, confirm pass**

```bash
cargo test --lib config
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat(config): add default roots for SwiftPM and Xcode caches"
```

---

## Task 11: Integration test — synthetic-fixture pipeline

**Files:**
- Create: `tests/integration_swiftpm_xcode.rs`

- [ ] **Step 1: Write the integration test**

```rust
// Integration test: synthetic SwiftPM + Xcode cache fixtures exercise the
// full detect → semantic_name → metadata → safety pipeline.
//
// E2E exempt per design spec: neither provider participates in OSV /
// version-check, so the standard "install tool, download vulnerable
// package" rubric does not apply.

use ccmd::providers::{self, SafetyLevel};
use ccmd::tree::node::CacheKind;
use std::fs;

#[test]
fn swiftpm_fixture_pipeline() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Library/Caches/org.swift.swiftpm");
    let repo = root.join("repositories/swift-collections-abc1234");
    let artifact = root.join("artifacts/MyBinaryDep");
    let manifest = root.join("manifests/deadbeef12345");
    fs::create_dir_all(&repo).unwrap();
    fs::create_dir_all(&artifact).unwrap();
    fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    fs::write(&manifest, b"").unwrap();

    // detect()
    assert_eq!(providers::detect(&root), CacheKind::SwiftPm);
    assert_eq!(providers::detect(&repo), CacheKind::SwiftPm);

    // semantic_name()
    assert_eq!(
        providers::semantic_name(CacheKind::SwiftPm, &repo),
        Some("swift-collections".into())
    );
    assert_eq!(
        providers::semantic_name(CacheKind::SwiftPm, &artifact),
        Some("MyBinaryDep".into())
    );

    // safety()
    assert_eq!(
        providers::safety(CacheKind::SwiftPm, &repo),
        SafetyLevel::Caution
    );
    assert_eq!(
        providers::safety(CacheKind::SwiftPm, &artifact),
        SafetyLevel::Safe
    );
    assert_eq!(
        providers::safety(CacheKind::SwiftPm, &manifest),
        SafetyLevel::Safe
    );
}

#[test]
fn xcode_fixture_pipeline() {
    let tmp = tempfile::tempdir().unwrap();
    let dd = tmp
        .path()
        .join("Library/Developer/Xcode/DerivedData/MyApp-abc123def");
    fs::create_dir_all(&dd).unwrap();
    let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>WORKSPACE_PATH</key>
    <string>/Users/j/dev/MyApp/MyApp.xcworkspace</string>
</dict>
</plist>"#;
    fs::write(dd.join("Info.plist"), plist).unwrap();

    let ds = tmp
        .path()
        .join("Library/Developer/Xcode/iOS DeviceSupport/17.4 (21E213)");
    fs::create_dir_all(&ds).unwrap();

    let sim = tmp
        .path()
        .join("Library/Developer/CoreSimulator/Caches/com.apple.Sim");
    fs::create_dir_all(&sim).unwrap();

    // detect
    assert_eq!(providers::detect(&dd), CacheKind::Xcode);
    assert_eq!(providers::detect(&ds), CacheKind::Xcode);
    assert_eq!(providers::detect(&sim), CacheKind::Xcode);

    // semantic_name
    assert_eq!(
        providers::semantic_name(CacheKind::Xcode, &dd),
        Some("MyApp.xcworkspace (at /Users/j/dev/MyApp/MyApp.xcworkspace)".into())
    );
    assert_eq!(
        providers::semantic_name(CacheKind::Xcode, &ds),
        Some("17.4 (21E213)".into())
    );
    assert_eq!(providers::semantic_name(CacheKind::Xcode, &sim), None);

    // safety
    assert_eq!(providers::safety(CacheKind::Xcode, &dd), SafetyLevel::Caution);
    assert_eq!(providers::safety(CacheKind::Xcode, &ds), SafetyLevel::Safe);
    assert_eq!(providers::safety(CacheKind::Xcode, &sim), SafetyLevel::Safe);
}
```

Note: if the crate's lib name differs from `ccmd`, adjust imports. (Verify via `cargo pkgid` or look at existing `tests/` files.)

- [ ] **Step 2: Run integration tests**

```bash
cargo test --test integration_swiftpm_xcode
```

Expected: pass. If import errors, check `Cargo.toml` for the library name and adjust.

- [ ] **Step 3: Commit**

```bash
git add tests/integration_swiftpm_xcode.rs
git commit -m "test: integration pipeline for SwiftPM + Xcode providers"
```

---

## Task 12: Docs — README, CHANGELOG, TODO

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`
- Modify: `TODO.md`

- [ ] **Step 1: Update README**

Add rows to the Supported Caches table (locate it via `grep -n "Supported Caches" README.md`) for SwiftPM and Xcode. Bump the provider count in the Features bullet.

- [ ] **Step 2: Update CHANGELOG**

Under `## [Unreleased]` / `### Added`:

```markdown
- **SwiftPM provider** — detects `~/Library/Caches/org.swift.swiftpm/` (macOS) and `~/.cache/org.swift.swiftpm/` (Linux); reports Caution safety for `repositories/` (re-clone cost), Safe for `artifacts/` and `manifests/`. No OSV or version-check (package-identity extraction is too brittle in v1 and OSV `SwiftURL` coverage is sparse); the `c` upgrade shortcut is a no-op for SwiftPM entries by design. (#11)
- **Xcode provider** — detects `~/Library/Developer/Xcode/DerivedData/`, `~/Library/Developer/Xcode/iOS DeviceSupport/`, and `~/Library/Developer/CoreSimulator/Caches/`. DerivedData is Caution (rebuild takes 5–30 min); the other two are Safe. Project dirs show the workspace path from `Info.plist` in the detail panel. No package identity (these are build artifacts, not packages). (#17)
```

- [ ] **Step 3: Update TODO.md**

Tick issues #11 and #17 (whatever the existing format is). Open the file first to see the format.

- [ ] **Step 4: Commit**

```bash
git add README.md CHANGELOG.md TODO.md
git commit -m "docs: add SwiftPM and Xcode provider entries"
```

---

## Task 13: Full verification + cleanup

- [ ] **Step 1: Run full test suite**

```bash
cargo test
cargo test --features mcp
```

Expected: all green.

- [ ] **Step 2: Clippy + fmt**

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 3: Package list sanity check**

```bash
cargo package --list --allow-dirty | grep -E "swiftpm|xcode"
```

Expected: the two new provider files listed, nothing else weird.

- [ ] **Step 4: Append lessons-learned entry to adding-a-provider.md**

Under `## 5. Lessons learned`, append:

```markdown
- **SwiftPM / Xcode (PR #?, v0.3.2?):** providers without package identity
  (disk-hygiene only) are a valid shape — return `None` from
  `package_id`, omit the OSV/registry arms, and document the Tier-3 E2E
  exemption in the module header comment so a future reviewer doesn't
  think the E2E test is missing by accident. Info.plist's XML format is
  stable enough to parse via string search; no `plist` crate needed.
```

- [ ] **Step 5: Final commit**

```bash
git add docs/adding-a-provider.md
git commit -m "docs: lessons-learned note for disk-hygiene-only providers"
```

- [ ] **Step 6: Push branch + open PR**

```bash
git push -u origin feat/swiftpm-xcode-providers
gh pr create --title "Add SwiftPM + Xcode DerivedData providers (#11, #17)" --body "$(cat <<'EOF'
## Summary
- Two new macOS-centric cache providers: **SwiftPM** (#11) and **Xcode DerivedData** (#17).
- SwiftPM classifies `repositories/` as Caution (re-clone), `artifacts/`+`manifests/` as Safe.
- Xcode DerivedData is Caution (rebuild cost 5–30 min); iOS DeviceSupport + CoreSimulator/Caches are Safe.
- Neither provider returns a `PackageId` — OSV, version-check, and upgrade-command are untouched (pure disk hygiene).
- Xcode DerivedData project dirs extract `WORKSPACE_PATH` from `Info.plist` for detail-panel display.

## Test plan
- [x] `cargo test` green (unit + integration)
- [x] `cargo clippy --all-targets -- -D warnings` clean
- [x] `cargo fmt --all -- --check` clean
- [x] L1 confusable-suffix guards on both providers
- [x] L2 non-ASCII handling in semantic_name / Info.plist
- [ ] Manual smoke: run `ccmd` on a machine with SwiftPM + Xcode populated caches

## Design
See `docs/superpowers/specs/2026-04-20-swiftpm-xcode-providers-design.md`.

## E2E exemption
Both providers are disk-hygiene-only — no OSV ecosystem, no registry, no upgrade command. The standard Tier-3 E2E rubric (install tool, download vulnerable package, verify OSV + version-check fire) does not apply. Coverage is Tier 1 (unit) + Tier 2 (integration with synthetic fixtures).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-Review Notes

- Spec coverage: every section of the spec maps to a task. ✓
- No placeholders. ✓
- Type consistency: `CacheKind::SwiftPm` and `CacheKind::Xcode` used consistently; function signatures match `maven.rs` reference. ✓
- Task granularity: 13 tasks, each 2–10 min of work. Multiple commits per provider to preserve bisectability. ✓
