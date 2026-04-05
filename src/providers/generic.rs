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
