//! Unified adapter projecting canonical built-in skills from [`crate::vault::builtin`].
//!
//! Eliminates duplicate skill definitions and hardcoded alias tables by deriving all
//! [`BuiltinSkill`] entries and trigger/alias lookups directly from the MiniVault catalog.

use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct BuiltinSkill {
    pub name: &'static str,
    pub description: &'static str,
    pub globs: &'static [&'static str],
    pub content: &'static str,
}

static BUILTIN_SKILLS_CACHE: OnceLock<Vec<BuiltinSkill>> = OnceLock::new();

pub fn get_all_builtin_skills() -> &'static [BuiltinSkill] {
    BUILTIN_SKILLS_CACHE.get_or_init(|| {
        crate::vault::builtin::get_all_builtin_skills()
            .into_iter()
            .map(|skill| {
                let globs: Vec<&'static str> = skill
                    .frontmatter
                    .globs
                    .into_iter()
                    .map(|g| &*Box::leak(g.into_boxed_str()))
                    .collect();
                BuiltinSkill {
                    name: Box::leak(skill.name.into_boxed_str()),
                    description: Box::leak(skill.description.into_boxed_str()),
                    globs: Box::leak(globs.into_boxed_slice()),
                    content: Box::leak(skill.raw_content.into_boxed_str()),
                }
            })
            .collect()
    })
}

pub fn find_builtin_skill(name: &str) -> Option<&'static BuiltinSkill> {
    let resolved = crate::vault::builtin::find_builtin_skill(name)?;
    get_all_builtin_skills()
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(&resolved.name))
}
