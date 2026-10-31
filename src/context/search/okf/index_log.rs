use super::OkfDocument;
use crate::error::Result;
use chrono::Utc;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

const INDEX_FILE: &str = "index.md";
const LOG_FILE: &str = "log.md";

/// Synthesizes a concise one-sentence summary for a directory in an OKF bundle (`bundle/synthesizer.py`).
pub fn synthesize_directory_description(_rel_path: &str, children: &[(String, String)]) -> String {
    if children.is_empty() {
        return String::new();
    }
    if children.len() == 1 && !children[0].1.trim().is_empty() {
        return children[0].1.trim().to_string();
    }
    let titles: Vec<&str> = children
        .iter()
        .map(|(t, _)| t.trim())
        .filter(|t| !t.is_empty())
        .collect();
    let joined = if titles.is_empty() {
        "no titled entries".to_string()
    } else {
        titles.join(", ")
    };
    format!("Contains {} entries: {}.", children.len(), joined)
}

/// Builds the markdown text for a single `index.md` file (`bundle/index.py:_build_index_text`).
/// Entries are `(type, title, relative_link, description)`.
pub fn build_index_text(
    entries: &[(String, String, String, String)],
    include_okf_version_header: bool,
) -> String {
    let mut grouped: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
    for (typ, title, link, desc) in entries {
        let key = if typ.trim().is_empty() {
            "Other".to_string()
        } else {
            typ.trim().to_string()
        };
        grouped
            .entry(key)
            .or_default()
            .push((title.clone(), link.clone(), desc.clone()));
    }

    let mut sections = Vec::new();
    for (typ, mut items) in grouped {
        items.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        let mut lines = vec![format!("# {}", typ), String::new()];
        for (title, link, desc) in items {
            let suffix = if desc.trim().is_empty() {
                String::new()
            } else {
                format!(" - {}", desc.trim())
            };
            lines.push(format!("* [{}]({}){}", title, link, suffix));
        }
        sections.push(lines.join("\n"));
    }

    let body = if sections.is_empty() {
        "# Index\n".to_string()
    } else {
        format!("{}\n", sections.join("\n\n"))
    };

    if include_okf_version_header {
        format!("---\nokf_version: \"0.2\"\n---\n\n{}", body)
    } else {
        body
    }
}

/// Collects all directories inside `bundle_root` that contain (directly or transitively) `.md` concept files.
fn directories_to_index(bundle_root: &Path) -> Vec<PathBuf> {
    let mut dirs = BTreeSet::new();
    collect_md_dirs_recursive(bundle_root, bundle_root, &mut dirs);

    let mut dir_list: Vec<PathBuf> = dirs.into_iter().collect();
    // Sort bottom-up: deepest relative path depth first, then lexicographically
    dir_list.sort_by(|a, b| {
        let depth_a = a
            .strip_prefix(bundle_root)
            .map(|p| p.components().count())
            .unwrap_or(0);
        let depth_b = b
            .strip_prefix(bundle_root)
            .map(|p| p.components().count())
            .unwrap_or(0);
        depth_b.cmp(&depth_a).then_with(|| a.cmp(b))
    });
    dir_list
}

fn collect_md_dirs_recursive(bundle_root: &Path, current: &Path, dirs: &mut BTreeSet<PathBuf>) {
    let Ok(read_dir) = fs::read_dir(current) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') {
                collect_md_dirs_recursive(bundle_root, &path, dirs);
            }
        } else if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if fname == INDEX_FILE || fname == LOG_FILE {
                continue;
            }
            let mut cur_opt = path.parent();
            while let Some(cur) = cur_opt {
                if !cur.starts_with(bundle_root) {
                    break;
                }
                dirs.insert(cur.to_path_buf());
                if cur == bundle_root {
                    break;
                }
                cur_opt = cur.parent();
            }
        }
    }
}

/// Walks `bundle_root` bottom-up and generates an OKF v0.2 §8 `index.md` in every directory (`bundle/index.py:regenerate_indexes`).
pub fn regenerate_indexes(
    bundle_root: &Path,
    include_root_version_header: bool,
) -> Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    if !bundle_root.exists() {
        return Ok(written);
    }

    let directories = directories_to_index(bundle_root);
    let mut dir_descriptions: HashMap<PathBuf, String> = HashMap::new();

    for directory in directories {
        let mut entries: Vec<(String, String, String, String)> = Vec::new();
        let Ok(read_dir) = fs::read_dir(&directory) else {
            continue;
        };

        let mut children: Vec<PathBuf> = read_dir.flatten().map(|e| e.path()).collect();
        children.sort();

        for child in children {
            let Some(child_name) = child.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if child_name == INDEX_FILE || child_name == LOG_FILE || child_name.starts_with('.') {
                continue;
            }

            if child.is_file() && child.extension().and_then(|e| e.to_str()) == Some("md") {
                let Ok(raw) = fs::read_to_string(&child) else {
                    continue;
                };
                let (fm_opt, body) = OkfDocument::parse(&raw).unwrap_or((None, &raw));
                let stem = child
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(child_name);

                let (typ, title, desc) = if let Some(fm) = fm_opt {
                    let t = fm.title.unwrap_or_else(|| stem.to_string());
                    let d = fm.description.unwrap_or_default();
                    (fm.concept_type, t, d)
                } else {
                    // Graceful fallback for markdown files without frontmatter
                    let heading_title = body
                        .lines()
                        .find(|l| l.trim_start().starts_with("# "))
                        .map(|l| l.trim_start().trim_start_matches("# ").trim().to_string())
                        .unwrap_or_else(|| stem.to_string());
                    let first_para = body
                        .lines()
                        .map(|l| l.trim())
                        .find(|l| {
                            !l.is_empty()
                                && !l.starts_with('#')
                                && !l.starts_with("---")
                                && !l.starts_with('>')
                        })
                        .unwrap_or("")
                        .chars()
                        .take(120)
                        .collect::<String>();
                    ("Document".to_string(), heading_title, first_para)
                };

                entries.push((typ, title, child_name.to_string(), desc));
            } else if child.is_dir() {
                if let Some(desc) = dir_descriptions.get(&child) {
                    entries.push((
                        "Subdirectories".to_string(),
                        child_name.to_string(),
                        format!("{}/{}", child_name, INDEX_FILE),
                        desc.clone(),
                    ));
                }
            }
        }

        if entries.is_empty() {
            continue;
        }

        let is_root = directory == bundle_root;
        let index_text = build_index_text(&entries, is_root && include_root_version_header);
        let index_path = directory.join(INDEX_FILE);
        fs::write(&index_path, &index_text).ok();
        written.push(index_path);

        if !is_root {
            let pairs: Vec<(String, String)> = entries
                .iter()
                .map(|(_, title, _, desc)| (title.clone(), desc.clone()))
                .collect();
            let rel = directory
                .strip_prefix(bundle_root)
                .unwrap_or(&directory)
                .to_string_lossy();
            let synth = synthesize_directory_description(&rel, &pairs);
            dir_descriptions.insert(directory, synth);
        }
    }

    Ok(written)
}

/// Appends a chronological update entry to `log.md` following OKF v0.2 §9:
/// Date-grouped entries under ISO 8601 `## YYYY-MM-DD` headings, newest first, with no frontmatter.
pub fn append_okf_log_entry(
    docs_dir: &Path,
    actor: &str,
    action: &str,
    target_file: &str,
    summary: &str,
) -> Result<()> {
    if !docs_dir.exists() {
        fs::create_dir_all(docs_dir).ok();
    }

    let log_path = docs_dir.join(LOG_FILE);
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let today_heading = format!("## {}", today);

    let action_title = format_action_label(action);
    let target_display = if target_file.trim().is_empty() {
        String::new()
    } else if target_file.ends_with(".md") {
        let link_path = if target_file.starts_with('/') || target_file.starts_with('.') {
            target_file.to_string()
        } else {
            format!("/{}", target_file)
        };
        format!(" ([{}]({}))", target_file, link_path)
    } else {
        format!(" (`{}`)", target_file)
    };
    let actor_suffix = if actor.trim().is_empty() {
        String::new()
    } else {
        format!(" — `{}`", actor.trim())
    };

    let bullet_line = format!(
        "* **{}**: {}{}{}",
        action_title,
        summary.trim(),
        target_display,
        actor_suffix
    );

    let existing = if log_path.exists() {
        fs::read_to_string(&log_path).unwrap_or_default()
    } else {
        String::new()
    };

    // Strip any legacy YAML frontmatter if present from older versions
    let body_only = if existing.trim_start().starts_with("---") {
        let (_, body) = OkfDocument::parse(&existing).unwrap_or((None, &existing));
        body.to_string()
    } else {
        existing
    };

    let mut lines: Vec<String> = body_only.lines().map(|l| l.to_string()).collect();

    // Ensure `# Directory Update Log` header at top
    let has_h1 = lines.iter().any(|l| l.trim().starts_with("# "));
    if !has_h1 {
        let new_content = format!(
            "# Directory Update Log\n\n{}\n{}\n",
            today_heading, bullet_line
        );
        fs::write(&log_path, new_content).ok();
        return Ok(());
    }

    if let Some(pos) = lines.iter().position(|l| l.trim() == today_heading) {
        // Insert newest entry immediately under today's `## YYYY-MM-DD` heading
        lines.insert(pos + 1, bullet_line);
    } else {
        // Insert new `## YYYY-MM-DD` section right before the first existing `## ` heading (or at end after `# `)
        if let Some(first_h2) = lines.iter().position(|l| l.trim().starts_with("## ")) {
            lines.insert(first_h2, String::new());
            lines.insert(first_h2, bullet_line);
            lines.insert(first_h2, today_heading);
        } else {
            if !lines.last().map(|l| l.trim().is_empty()).unwrap_or(true) {
                lines.push(String::new());
            }
            lines.push(today_heading);
            lines.push(bullet_line);
        }
    }

    let mut final_text = lines.join("\n");
    if !final_text.ends_with('\n') {
        final_text.push('\n');
    }
    fs::write(&log_path, final_text).ok();
    Ok(())
}

fn format_action_label(action: &str) -> String {
    let clean = action.trim();
    if clean.is_empty() {
        return "Update".to_string();
    }
    match clean.to_uppercase().as_str() {
        "SYNC" => "Sync".to_string(),
        "CREATE" | "CREATION" => "Creation".to_string(),
        "UPDATE" => "Update".to_string(),
        "DELETE" | "DEPRECATE" | "DEPRECATION" => "Deprecation".to_string(),
        "VERIFY" | "VERIFICATION" => "Verification".to_string(),
        _ => {
            let mut chars = clean.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => "Update".to_string(),
            }
        }
    }
}
