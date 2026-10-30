pub mod backup;
pub mod store;
pub mod transaction;
pub mod undo;

/// Atomically writes a JSON manifest using a temporary file and rename to prevent torn writes.
pub(crate) fn write_json_atomic<T: serde::Serialize>(
    manifest_path: &std::path::Path,
    value: &T,
) -> crate::error::Result<()> {
    let json_data = serde_json::to_string_pretty(value)?;
    let tmp_path = manifest_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json_data)?;
    if std::fs::rename(&tmp_path, manifest_path).is_err() {
        let _ = std::fs::remove_file(&tmp_path);
        std::fs::write(manifest_path, json_data)?;
    }
    Ok(())
}

/// Parses JSON content and self-heals trailing `}` characters if a legacy torn write occurred.
pub(crate) fn parse_json_lenient<T: serde::de::DeserializeOwned + serde::Serialize>(
    raw_content: &str,
    heal_path: Option<&std::path::Path>,
) -> crate::error::Result<T> {
    match serde_json::from_str::<T>(raw_content) {
        Ok(val) => Ok(val),
        Err(first_err) => {
            let mut trimmed = raw_content.trim().to_string();
            for _ in 0..4 {
                if trimmed.ends_with('}') {
                    trimmed.pop();
                    trimmed = trimmed.trim_end().to_string();
                    if let Ok(healed) = serde_json::from_str::<T>(&trimmed) {
                        if let Some(p) = heal_path {
                            tracing::warn!(
                                path = %p.display(),
                                "Self-healed JSON manifest with trailing characters"
                            );
                            let _ = write_json_atomic(p, &healed);
                        }
                        return Ok(healed);
                    }
                } else {
                    break;
                }
            }
            Err(first_err.into())
        }
    }
}
