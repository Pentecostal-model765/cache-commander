// Integration test: synthetic SwiftPM + Xcode cache fixtures exercise the
// full detect → semantic_name → metadata → safety pipeline.
//
// Tier-3 E2E exempt per design spec: neither provider participates in
// OSV / version-check, so the standard "install tool, download
// vulnerable package" rubric does not apply. Tier 1 (in-module unit
// tests) + this Tier 2 integration test cover the disk-hygiene
// behavior end to end.

use ccmd::providers::{self, SafetyLevel};
use ccmd::tree::node::CacheKind;
use std::fs;

#[test]
fn swiftpm_fixture_pipeline() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Library/Caches/org.swift.swiftpm");
    let repo = root.join("repositories/swift-collections-abc1234");
    let artifact = root.join("artifacts/MyBinaryDep");
    let manifest_dir = root.join("manifests");
    let manifest = manifest_dir.join("deadbeef12345");
    fs::create_dir_all(&repo).unwrap();
    fs::create_dir_all(&artifact).unwrap();
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(&manifest, b"").unwrap();

    // detect
    assert_eq!(providers::detect(&root), CacheKind::SwiftPm);
    assert_eq!(providers::detect(&repo), CacheKind::SwiftPm);
    assert_eq!(providers::detect(&manifest), CacheKind::SwiftPm);

    // semantic_name
    assert_eq!(
        providers::semantic_name(CacheKind::SwiftPm, &repo),
        Some("swift-collections".into())
    );
    assert_eq!(
        providers::semantic_name(CacheKind::SwiftPm, &artifact),
        Some("MyBinaryDep".into())
    );
    assert_eq!(
        providers::semantic_name(CacheKind::SwiftPm, &manifest),
        None
    );

    // metadata surfaces Contents on the known roots
    let repos_root = root.join("repositories");
    assert!(
        providers::metadata(CacheKind::SwiftPm, &repos_root)
            .iter()
            .any(|f| f.label == "Contents"),
        "expected Contents metadata on repositories/"
    );

    // safety
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

    // detect recognises all three root subtrees.
    assert_eq!(providers::detect(&dd), CacheKind::Xcode);
    assert_eq!(providers::detect(&ds), CacheKind::Xcode);
    assert_eq!(providers::detect(&sim), CacheKind::Xcode);

    // semantic_name extracts WORKSPACE_PATH from Info.plist; falls back
    // for DeviceSupport (dir name); returns None for opaque simulator
    // caches.
    assert_eq!(
        providers::semantic_name(CacheKind::Xcode, &dd),
        Some("MyApp.xcworkspace (at /Users/j/dev/MyApp/MyApp.xcworkspace)".into())
    );
    assert_eq!(
        providers::semantic_name(CacheKind::Xcode, &ds),
        Some("17.4 (21E213)".into())
    );
    assert_eq!(providers::semantic_name(CacheKind::Xcode, &sim), None);

    // metadata: DerivedData project dir surfaces the workspace path.
    let fields = providers::metadata(CacheKind::Xcode, &dd);
    assert!(
        fields
            .iter()
            .any(|f| f.label == "Workspace" && f.value == "/Users/j/dev/MyApp/MyApp.xcworkspace"),
        "expected Workspace metadata, got {fields:?}"
    );

    // safety classifies DerivedData as Caution (rebuild cost); others Safe.
    assert_eq!(
        providers::safety(CacheKind::Xcode, &dd),
        SafetyLevel::Caution
    );
    assert_eq!(providers::safety(CacheKind::Xcode, &ds), SafetyLevel::Safe);
    assert_eq!(providers::safety(CacheKind::Xcode, &sim), SafetyLevel::Safe);
}
