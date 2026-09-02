use std::process::Command;

/// Native multi-runtime environment health diagnostics.
pub struct OnpkgDoctor;

impl OnpkgDoctor {
    /// Inspects all runtime tools and package managers on the current system.
    pub fn diagnose() -> String {
        let tools = [
            ("Bun", "bun", "--version"),
            ("UV (Python)", "uv", "--version"),
            ("Node.js", "node", "--version"),
            ("Python 3", "python3", "--version"),
            ("Rust / Cargo", "cargo", "--version"),
            ("Flutter", "flutter", "--version"),
            ("npm", "npm", "--version"),
            ("pnpm", "pnpm", "--version"),
            ("yarn", "yarn", "--version"),
            ("Git", "git", "--version"),
            ("GitHub CLI", "gh", "--version"),
        ];

        let mut res = String::from("🩺 **minicode + onpkg Multi-Runtime Diagnostics**\n\n");
        res.push_str("| Runtime / Tool | Status | Version / Details |\n");
        res.push_str("| :--- | :---: | :--- |\n");

        for (name, bin, flag) in tools {
            match Command::new(bin).arg(flag).output() {
                Ok(output) if output.status.success() => {
                    let ver = String::from_utf8_lossy(&output.stdout);
                    let first_line = ver.lines().next().unwrap_or("Installed").trim();
                    let compact_ver: String = if first_line.chars().count() > 40 {
                        first_line.chars().take(40).collect()
                    } else {
                        first_line.to_string()
                    };
                    res.push_str(&format!("| **{}** | ✔ | `{}` |\n", name, compact_ver));
                }
                _ => {
                    res.push_str(&format!("| **{}** | ○ | *Not found in PATH* |\n", name));
                }
            }
        }

        // OS sandbox status
        res.push_str("\n| **OS Sandbox** |");
        if crate::sandbox::landlock::is_landlock_supported() {
            res.push_str(" ✔ | Landlock available — path enforcement active |\n");
        } else {
            #[cfg(target_os = "linux")]
            {
                res.push_str(" ⚠ | Landlock unavailable (kernel < 5.13) — path checks only |\n");
            }
            #[cfg(not(target_os = "linux"))]
            {
                res.push_str(" ○ | Landlock not supported on this OS |\n");
            }
        }

        res
    }
}
