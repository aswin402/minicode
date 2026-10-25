use crate::config::Config;
use crate::error::Result;
use std::path::Path;

pub struct ConfigMenu;

impl ConfigMenu {
    pub fn save_all(config: &Config, workspace: &Path) -> Result<()> {
        Self::save_all_with_dirs(config, workspace, None)
    }

    pub fn save_all_with_dirs(
        config: &Config,
        workspace: &Path,
        custom_config_dir: Option<&Path>,
    ) -> Result<()> {
        // 1. Determine target config directory
        let app_dir = if let Some(dir) = custom_config_dir {
            Some(dir.to_path_buf())
        } else {
            dirs::config_dir().map(|d| d.join(crate::constants::CONFIG_DIR_NAME))
        };

        if let Some(app_dir) = app_dir {
            let _ = std::fs::create_dir_all(&app_dir);
            let toml_path = app_dir.join(crate::constants::CONFIG_FILE_NAME);

            // Non-destructively merge with existing configuration to preserve user themes & preferences
            let new_val = toml::Value::try_from(config.clone())
                .unwrap_or_else(|_| toml::Value::Table(toml::map::Map::new()));
            let merged_value = if let Ok(content) = std::fs::read_to_string(&toml_path) {
                if let Ok(mut existing_val) = toml::from_str::<toml::Value>(&content) {
                    merge_toml_values(&mut existing_val, &new_val);
                    existing_val
                } else {
                    new_val
                }
            } else {
                new_val
            };

            let content = toml::to_string_pretty(&merged_value)
                .map_err(crate::error::ConfigError::TomlSerialize)?;
            let _ = std::fs::write(&toml_path, content);

            // Also write all configured API keys to global/custom .env
            let global_env_path = app_dir.join(crate::constants::ENV_FILE_NAME);
            for (p, k) in &config.provider.api_keys {
                let env_name_str = crate::ui::setup::SetupWizard::custom_env_var(p);
                let _ = update_dotenv_file(&global_env_path, &env_name_str, k);
            }
        }

        // 2. Also update workspace .env if workspace exists
        let env_path = workspace.join(crate::constants::ENV_FILE_NAME);
        let _ = update_dotenv_file(
            &env_path,
            "MINICODE_APPROVAL_POLICY",
            &config.agent.approval_policy,
        );

        Ok(())
    }
}

/// Helper that deeply merges TOML tables without overwriting established custom user settings with defaults
fn merge_toml_values(target: &mut toml::Value, source: &toml::Value) {
    if let (toml::Value::Table(target_table), toml::Value::Table(source_table)) =
        (&mut *target, source)
    {
        for (k, v) in source_table {
            if let Some(target_entry) = target_table.get_mut(k) {
                // If the source value is just a generic default or empty, do not clobber user's custom settings
                let is_default_or_empty = matches!(
                    (k.as_str(), v.as_str()),
                    ("theme", Some("auto"))
                        | ("animation", Some("dual_pillars"))
                        | ("todo_style", Some("tree"))
                        | ("swarm_style", Some("stylish"))
                        | ("toast_style", Some("opencode"))
                        | ("activity_style", Some("action_detail"))
                        | ("default", Some(""))
                        | ("model", Some(""))
                );
                if !is_default_or_empty {
                    merge_toml_values(target_entry, v);
                }
            } else {
                target_table.insert(k.clone(), v.clone());
            }
        }
    } else {
        *target = source.clone();
    }
}

pub fn update_dotenv_file(env_path: &Path, key: &str, value: &str) -> Result<()> {
    if let Some(parent) = env_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut lines = Vec::new();
    let mut found = false;
    let prefix = format!("{}=", key);
    let replacement = format!("{}={}", key, value);

    if env_path.exists() {
        if let Ok(content) = std::fs::read_to_string(env_path) {
            for line in content.lines() {
                if line.starts_with(&prefix) {
                    lines.push(replacement.clone());
                    found = true;
                } else {
                    lines.push(line.to_string());
                }
            }
        }
    }

    if !found {
        lines.push(replacement);
    }

    std::fs::write(env_path, lines.join("\n") + "\n").map_err(|e| {
        crate::error::ConfigError::FileWrite {
            path: env_path.display().to_string(),
            source: e,
        }
        .into()
    })
}
