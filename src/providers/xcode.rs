// Xcode provider (DerivedData, iOS DeviceSupport, CoreSimulator Caches).
//
// No package identity / OSV / version-check / upgrade-command: these are
// build artifacts, not packages. Tier-3 E2E tests intentionally exempt
// (see design spec 2026-04-20-swiftpm-xcode-providers-design.md).

use super::MetadataField;
use std::path::Path;

pub fn semantic_name(_path: &Path) -> Option<String> {
    None
}

pub fn metadata(_path: &Path) -> Vec<MetadataField> {
    Vec::new()
}
