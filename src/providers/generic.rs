use super::MetadataField;
use std::path::Path;

pub fn metadata(path: &Path) -> Vec<MetadataField> {
    let mut fields = Vec::new();

    if path.is_dir()
        && let Ok(entries) = std::fs::read_dir(path)
    {
        let count = entries.filter_map(|e| e.ok()).count();
        fields.push(MetadataField {
            label: "Contents".to_string(),
            value: format!("{count} items"),
        });
    }

    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_reports_entry_count_for_real_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a"), "").unwrap();
        std::fs::write(tmp.path().join("b"), "").unwrap();
        std::fs::create_dir(tmp.path().join("c")).unwrap();
        let fields = metadata(tmp.path());
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].label, "Contents");
        assert_eq!(fields[0].value, "3 items");
    }

    #[test]
    fn metadata_empty_dir_reports_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let fields = metadata(tmp.path());
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].value, "0 items");
    }

    #[test]
    fn metadata_returns_empty_for_nonexistent_path() {
        let fields = metadata(std::path::Path::new("/does/not/exist/xyz"));
        assert!(fields.is_empty());
    }
}
