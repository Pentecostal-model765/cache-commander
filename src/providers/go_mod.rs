// Go module cache + build cache provider.
//
// Two logical caches under one CacheKind::Go:
// - Module cache (`$GOMODCACHE`, default `~/go/pkg/mod`): Safe.
//   Tarballs at `cache/download/<module>/@v/<version>.zip` plus extracted
//   copies at `pkg/mod/<module>@<version>/`. Go chmod -w's the extracted
//   tree, which is why this provider ships a pre_delete hook.
// - Build cache (`$GOCACHE`, default `~/Library/Caches/go-build` on
//   macOS, `~/.cache/go-build` on Linux): Caution (cold rebuild cost).

use super::MetadataField;
use std::path::Path;

pub fn semantic_name(_path: &Path) -> Option<String> {
    None
}

pub fn metadata(_path: &Path) -> Vec<MetadataField> {
    Vec::new()
}

pub fn package_id(_path: &Path) -> Option<super::PackageId> {
    None
}

pub fn pre_delete(_path: &Path) -> Result<(), String> {
    Ok(())
}
