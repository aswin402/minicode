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
                    let compact_ver = if first_line.len() > 40 {
                        &first_line[..40]
                    } else {
                        first_line
                    };
                    res.push_str(&format!("| **{}** | ✔ | `{}` |\n", name, compact_ver));
                }
                _ => {
                    res.push_str(&format!("| **{}** | ○ | *Not found in PATH* |\n", name));
                }
            }
        }

        res
    }
}
