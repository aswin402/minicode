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
        // 1. Determine target global config directory (honoring custom_config_dir & MINICODE_CONFIG_DIR)
        let app_dir = if let Some(dir) = custom_config_dir {
            Some(dir.to_path_buf())
        } else {
            Config::resolve_global_config_dir()
        };

        if let Some(app_dir) = app_dir {
            let _ = std::fs::create_dir_all(&app_dir);
            let toml_path = app_dir.join(crate::constants::CONFIG_FILE_NAME);

            // Use the canonical atomic writer so global config.toml preserves existing
            // credential/endpoint maps while cleanly updating provider/model/preferences.
            config.write_config_to_path_with_options(&toml_path, false)?;
            config.sync_global_dotenv(&app_dir);
        }

        // 2. If a project-local `.minicode/config.toml` already exists in the current workspace,
        //    keep it updated (with plaintext secrets stripped so credentials stay global-only).
        let local_cfg = workspace
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join(crate::constants::CONFIG_FILE_NAME);
        if local_cfg.exists() {
            let _ = config.save_workspace(workspace);
        }

        // 3. Only update workspace `.env` if one already exists (avoid creating stray `.env` files).
        let env_path = workspace.join(crate::constants::ENV_FILE_NAME);
        if env_path.exists() {
            let _ = update_dotenv_file(
                &env_path,
                "MINICODE_APPROVAL_POLICY",
                &config.agent.approval_policy,
            );
        }

        Ok(())
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
