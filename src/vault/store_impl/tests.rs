//! Unit tests for [`VaultStore`].

use super::*;
use crate::vault::models::SkillKind;
use tempfile::tempdir;

#[test]
fn test_vault_store_crud_lifecycle() {
    let dir = tempdir().unwrap();
    let ws = dir.path();
    let store = VaultStore::new(ws);

    // 1. List contains built-in skills
    let list = store.list_all_skills();
    assert!(list.len() >= 15);
    assert!(list.iter().any(|s| s.name == "tailwind-v4"));

    // 2. Load built-in skill to project
    let loaded = store.load_to_project("tailwind-v4").unwrap();
    assert_eq!(loaded.scope, SkillScope::Project);
    assert!(loaded.is_active_in_project);
    assert!(store.project_skills_dir().join("tailwind-v4").exists());

    // 3. Create custom project skill
    let custom = store
        .create_skill(
            SkillScope::Project,
            "custom-graphql",
            "Project specific GraphQL guidelines",
            Some("api"),
            Some("reference"),
            "Always use Apollo Client with cache-and-network.",
            vec!["graphql".to_string()],
            vec!["*.graphql".to_string()],
        )
        .unwrap();

    assert_eq!(custom.name, "custom-graphql");
    assert_eq!(custom.scope, SkillScope::Project);
    assert_eq!(custom.category(), "api");
    assert_eq!(custom.kind(), SkillKind::Reference);

    // 4. Search
    let search_res = store.search_skills("graphql");
    assert_eq!(search_res.len(), 1);
    assert_eq!(search_res[0].name, "custom-graphql");

    let search_cat = store.search_skills_categorized("", Some("api"));
    assert_eq!(search_cat.len(), 1);
    assert_eq!(search_cat[0].name, "custom-graphql");

    // 5. Update
    let updated = store
        .update_skill(
            SkillScope::Project,
            "custom-graphql",
            "Updated: Use URQL instead of Apollo.",
        )
        .unwrap();
    assert!(updated.instructions.contains("Use URQL instead of Apollo."));

    // 6. Unload from project
    store.unload_from_project("custom-graphql").unwrap();
    assert!(store.get_skill("custom-graphql").is_err());
}

#[test]
fn test_vault_store_bundles_lifecycle() {
    let dir = tempdir().unwrap();
    let ws = dir.path();
    let store = VaultStore::new(ws);

    // 1. List includes built-in bundles
    let bundles = store.list_all_bundles();
    assert!(bundles.len() >= 6);
    assert!(bundles.iter().any(|b| b.name == "fullstack-nextjs"));

    // 2. Load bundle to project
    let (loaded, failed) = store.load_bundle_to_project("fullstack-nextjs").unwrap();
    assert!(failed.is_empty());
    assert!(loaded.contains(&"react".to_string()));
    assert!(loaded.contains(&"nextjs".to_string()));
    assert!(loaded.contains(&"tailwind-v4".to_string()));

    // Check project skills dir
    assert!(store.project_skills_dir().join("react").exists());
    assert!(store.project_skills_dir().join("nextjs").exists());

    // 3. Create custom bundle
    let custom_bundle = store
        .create_bundle(
            SkillScope::Project,
            "custom-stack",
            "Custom team stack",
            "team",
            vec!["react".to_string(), "tailwind-v4".to_string()],
            vec!["custom".to_string()],
        )
        .unwrap();

    assert_eq!(custom_bundle.name, "custom-stack");
    assert_eq!(custom_bundle.scope, SkillScope::Project);
    assert!(store.get_bundle("custom-stack").is_some());

    // 4. Unload bundle
    let unloaded = store
        .unload_bundle_from_project("fullstack-nextjs")
        .unwrap();
    assert!(unloaded.contains(&"react".to_string()));
}

#[test]
fn test_vault_sources_and_gotchas_lifecycle() {
    let dir = tempdir().unwrap();
    let ws = dir.path();
    let store = VaultStore::new(ws).with_custom_global_vault(ws.join("test_global_vault"));

    // 1. Sources CRUD
    let src = LearnedSource {
        id: "src-1".to_string(),
        uri: "https://docs.rs/tokio".to_string(),
        title: "Tokio Async Runtime".to_string(),
        kind: SourceKind::WebDoc,
        summary: "Tokio async documentation".to_string(),
        tags: vec!["tokio".to_string(), "rust".to_string(), "async".to_string()],
        extracted_skill_or_doc: Some("tokio".to_string()),
        created_at: "2026-09-29T10:00:00Z".to_string(),
        last_referenced: "2026-09-29T10:00:00Z".to_string(),
    };

    store.add_or_update_source(src.clone()).unwrap();
    let sources = store.load_sources();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].title, "Tokio Async Runtime");

    let search_res = store.search_sources("tokio");
    assert_eq!(search_res.len(), 1);
    let search_empty = store.search_sources("nonexistent");
    assert!(search_empty.is_empty());

    // 2. Gotchas CRUD and Occurrence Bumping
    let gotcha = GlobalGotcha {
        id: "gotcha-1".to_string(),
        trigger: "tokio::spawn inside non-async scope".to_string(),
        context_scope: "Rust/Tokio".to_string(),
        failed_attempt: "Calling tokio::spawn without runtime handle".to_string(),
        verified_fix: "Use tokio::runtime::Handle::current().spawn or mark fn async".to_string(),
        occurrence_count: 1,
        created_at: "2026-09-29T10:00:00Z".to_string(),
    };

    store.add_or_update_global_gotcha(gotcha.clone()).unwrap();
    let gotchas = store.load_global_gotchas();
    assert_eq!(gotchas.len(), 1);
    assert_eq!(gotchas[0].occurrence_count, 1);

    // Add the exact same trigger again to test automatic occurrence increment
    let duplicate_gotcha = GlobalGotcha {
        id: "gotcha-2".to_string(),
        trigger: "tokio::spawn inside non-async scope".to_string(),
        context_scope: "Rust/Tokio".to_string(),
        failed_attempt: "Another failure".to_string(),
        verified_fix: "Updated fix".to_string(),
        occurrence_count: 1,
        created_at: "2026-09-29T11:00:00Z".to_string(),
    };

    store.add_or_update_global_gotcha(duplicate_gotcha).unwrap();
    let updated_gotchas = store.load_global_gotchas();
    assert_eq!(updated_gotchas.len(), 1);
    assert_eq!(updated_gotchas[0].occurrence_count, 2);
    assert_eq!(updated_gotchas[0].verified_fix, "Updated fix");

    // Test relevant gotchas retrieval
    let relevant = store.find_relevant_gotchas("tokio async concurrency");
    assert_eq!(relevant.len(), 1);
    assert_eq!(relevant[0].occurrence_count, 2);

    let irrelevant = store.find_relevant_gotchas("react tailwind css");
    assert!(irrelevant.is_empty());
}

#[tokio::test]
async fn test_vault_ingest_local_repo() {
    let dir = tempdir().unwrap();
    let ws = dir.path();
    let store = VaultStore::new(ws).with_custom_global_vault(ws.join("test_global_vault"));

    // Create a fake local repo
    let repo_dir = dir.path().join("fake-repo");
    fs::create_dir_all(&repo_dir).unwrap();
    let readme_content = "# Fake Architecture\n\nThis is a pattern repository.";
    fs::write(repo_dir.join("README.md"), readme_content).unwrap();

    let (src, skill) = store
        .ingest_source(
            &repo_dir.to_string_lossy(),
            Some("fake-arch"),
            SkillScope::Project,
            None,
        )
        .await
        .unwrap();

    assert_eq!(src.kind, SourceKind::RepoTemplate);
    assert_eq!(src.title, "fake-arch");
    assert!(skill.is_some());
    let s = skill.unwrap();
    assert_eq!(s.name, "fake-arch");
    assert!(s.instructions.contains("This is a pattern repository."));

    // Verify stored in sources.json
    let sources = store.load_sources();
    assert!(sources.iter().any(|s| s.title == "fake-arch"));
}

#[test]
fn test_vault_workspace_matching_and_prompt_assembly() {
    let dir = tempdir().unwrap();
    let ws = dir.path();
    let store = VaultStore::new(ws);

    // 1. Initially empty directory
    let initial_tokens = store.detect_workspace_package_tokens();
    assert!(initial_tokens.is_empty());

    // 2. Add package.json with react and tailwindcss
    let pkg_json = r#"{
        "name": "my-app",
        "dependencies": {
            "react": "^19.0.0",
            "react-dom": "^19.0.0",
            "@tailwindcss/vite": "^4.0.0"
        }
    }"#;
    fs::write(ws.join("package.json"), pkg_json).unwrap();

    let tokens = store.detect_workspace_package_tokens();
    assert!(tokens.contains("react"));
    assert!(tokens.contains("tailwindcss") || tokens.contains("tailwind"));
    assert!(tokens.contains("vite"));

    // 3. Match skills for context based on stack
    let matched = store.match_skills_for_context::<&str>(None, None, &[]);
    let matched_names: Vec<_> = matched.iter().map(|s| s.name.as_str()).collect();
    assert!(matched_names.contains(&"react"));
    assert!(matched_names.contains(&"tailwind-v4") || matched_names.contains(&"tailwind"));

    // 4. Prompt matching for methodology skill (e.g. systematic-debugging when user says "fix this bug")
    let debug_matched = store.match_skills_for_context::<&str>(
        Some("Please fix this bug in user authentication"),
        None,
        &[],
    );
    let debug_names: Vec<_> = debug_matched.iter().map(|s| s.name.as_str()).collect();
    assert!(debug_names.contains(&"systematic-debugging"));
    assert!(debug_names.contains(&"security-audit"));

    // 5. Test format_prompt_skills_context contains full invariants
    let prompt_context = store.format_prompt_skills_context::<&str>(None, None, &[]);
    assert!(prompt_context.contains("# Active Technology & Methodology Skills:"));
    assert!(prompt_context.contains("Skill `react`"));
    assert!(prompt_context.contains("Functional Components"));
    assert!(prompt_context.contains("# Available Domain Skills in Vault:"));

    // 6. Test dynamic turn activation formatting
    let dynamic_block = store.format_dynamic_turn_skills::<&str>(
        Some("Help me write unit tests"),
        None,
        &[],
        &["react".to_string()],
    );
    assert!(dynamic_block.is_some());
    let dyn_str = dynamic_block.unwrap();
    assert!(dyn_str.contains("<auto_activated_skills>"));
    assert!(dyn_str.contains("Skill `tdd-workflow`"));
}

#[test]
fn test_level3_progressive_disclosure_and_allowed_tools() {
    let dir = tempdir().unwrap();
    let ws = dir.path();
    let global_vault = ws.join("global_vault");
    let store = VaultStore::new(ws).with_custom_global_vault(global_vault.clone());

    // Create a global skill with bundled scripts/ and references/
    let created = store
        .create_skill(
            SkillScope::Global,
            "custom-audit",
            "Audit API endpoints and run schema verification scripts",
            Some("security"),
            Some("workflow"),
            "Run `scripts/check.py` and consult `references/guide.md`.",
            vec![],
            vec!["*.py".to_string()],
        )
        .unwrap();

    // Auto-inferred triggers should be populated from name/description
    assert!(!created.frontmatter.triggers.is_empty());
    assert!(created
        .frontmatter
        .triggers
        .contains(&"custom-audit".to_string()));

    let global_skill_dir = global_vault.join("skills").join("custom-audit");
    fs::create_dir_all(global_skill_dir.join("scripts")).unwrap();
    fs::create_dir_all(global_skill_dir.join("references")).unwrap();
    fs::write(
        global_skill_dir.join("scripts").join("check.py"),
        "print('ok')",
    )
    .unwrap();
    fs::write(
        global_skill_dir.join("references").join("guide.md"),
        "# Reference Guide",
    )
    .unwrap();

    let fetched = store.get_skill("custom-audit").unwrap();
    let resources = fetched.discover_bundled_resources();
    assert_eq!(
        resources,
        vec![
            "references/guide.md".to_string(),
            "scripts/check.py".to_string()
        ]
    );

    // Load into project and verify Level-3 subdirectories are copied
    let loaded = store.load_to_project("custom-audit").unwrap();
    assert_eq!(loaded.discover_bundled_resources().len(), 2);

    // Verify allowed_tools collection for active skills (e.g. mcp-builder)
    let allowed = store.collect_active_skill_allowed_tools::<&str>(
        Some("build an mcp server with fastmcp"),
        None,
        &[],
    );
    assert!(allowed.contains(&"exec_cmd".to_string()));

    // Verify GitHub URL & anthropics/skills shorthand normalization
    assert_eq!(
        VaultStore::normalize_skill_import_url("anthropics/skills/mcp-builder"),
        "https://raw.githubusercontent.com/anthropics/skills/main/skills/mcp-builder/SKILL.md"
    );
    assert_eq!(
        VaultStore::normalize_skill_import_url(
            "https://github.com/anthropics/skills/tree/main/skills/webapp-testing"
        ),
        "https://raw.githubusercontent.com/anthropics/skills/main/skills/webapp-testing/SKILL.md"
    );
}

#[test]
fn test_vault_full_crud_on_builtin_and_orchestrator_skills() {
    let dir = tempdir().unwrap();
    let ws = dir.path();
    let store = VaultStore::new(ws);

    // 1. Read built-in orchestrator skill (`ask-user-inquiry`)
    let ask_skill = store.get_skill("ask-user-inquiry").unwrap();
    assert_eq!(ask_skill.scope, SkillScope::Builtin);
    assert_eq!(ask_skill.category(), "orchestrator");

    // 2. Surgically patch built-in orchestrator skill via patch_skill_full
    let patched = store
        .patch_skill_full(
            SkillScope::Project,
            "ask-user-inquiry",
            None,
            Some("4 to 6 distinct, well-explained options"),
            Some("4 to 6 distinct, well-explained options (Team Custom Rule)"),
            Some("Use when clarifying product scope with team custom batching rule"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(patched.scope, SkillScope::Project);
    assert!(patched
        .instructions
        .contains("4 to 6 distinct, well-explained options (Team Custom Rule)"));
    assert_eq!(
        patched.description,
        "Use when clarifying product scope with team custom batching rule"
    );

    // Verify MiniPowerEngine::format_brainstorm_prompt dynamically reflects the CRUD update
    let brainstorm_prompt =
        crate::agent::minipower::MiniPowerEngine::format_brainstorm_prompt(ws, "Custom OAuth Flow");
    assert!(
        brainstorm_prompt.contains("4 to 6 distinct, well-explained options (Team Custom Rule)")
    );

    // 3. Delete built-in orchestrator skill (`ask-user-inquiry`) and verify tombstone suppresses it
    store
        .delete_skill(SkillScope::Project, "ask-user-inquiry")
        .unwrap();
    assert!(store.get_skill("ask-user-inquiry").is_err());
    assert!(!store
        .list_all_skills()
        .iter()
        .any(|s| s.name == "ask-user-inquiry"));

    // 4. Restore deleted built-in skill via load_to_project
    let restored = store.load_to_project("ask-user-inquiry").unwrap();
    assert_eq!(restored.name, "ask-user-inquiry");
    assert!(store.get_skill("ask-user-inquiry").is_ok());
}

#[test]
fn test_sync_project_active_skills_and_level3_resource_reading() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    fs::write(
        ws.join("minikit.json"),
        r#"{"name": "minitest28", "stack": "react", "runtime": "bun", "packages": ["react", "react-dom"], "dev_packages": ["@tailwindcss/vite", "vite"], "active_skills": ["frontend-design", "ui-ux-pro-max"]}"#,
    )
    .unwrap();
    fs::create_dir_all(ws.join("src")).unwrap();
    fs::write(
        ws.join("src/App.tsx"),
        "export function App() { return <div />; }",
    )
    .unwrap();
    fs::write(ws.join("src/index.css"), "@import \"tailwindcss\";").unwrap();
    fs::create_dir_all(ws.join("minikit_docs/core")).unwrap();
    fs::write(ws.join("minikit_docs/core/spec.md"), "# Spec").unwrap();
    fs::write(ws.join("minikit_docs/core/todo.md"), "# Todo").unwrap();

    let store = VaultStore::new(ws);
    let workspace_files = vec![
        ws.join("minikit.json"),
        ws.join("src/App.tsx"),
        ws.join("src/index.css"),
        ws.join("minikit_docs/core/spec.md"),
        ws.join("minikit_docs/core/todo.md"),
    ];
    let synced = store.sync_project_active_skills(&workspace_files);

    for expected in [
        "ask-user-inquiry",
        "brainstorming",
        "executing-plans",
        "frontend-design",
        "miniblocks-ui",
        "minikit-scaffolding",
        "react",
        "tailwind-v4",
        "todo-planning",
        "ui-ux-pro-max",
        "using-minipower",
        "verification-before-completion",
        "vite",
        "writing-plans",
    ] {
        assert!(
            synced.contains(&expected.to_string()),
            "Expected skill '{}' in synced active_skills: {:?}",
            expected,
            synced
        );
    }

    // Unrelated or efficiency skills must not be auto-activated
    assert!(!synced.contains(&"caveman".to_string()));
    assert!(!synced.contains(&"canvas-design".to_string()));
    assert!(!synced.contains(&"rust-tokio".to_string()));

    // Verify minikit.json on disk was updated
    let manifest_active = store.get_manifest_active_skills();
    assert_eq!(manifest_active, synced);

    // Verify Level-3 bundled resource reading and path traversal protection
    let created = store
        .create_skill(
            SkillScope::Project,
            "custom-level3",
            "Custom Level-3 skill",
            Some("workflow"),
            Some("workflow"),
            "Check `references/api.md`.",
            vec!["level3".to_string()],
            vec![],
        )
        .unwrap();
    let skill_dir = created.path.as_ref().unwrap().parent().unwrap();
    fs::create_dir_all(skill_dir.join("references")).unwrap();
    fs::write(skill_dir.join("references/api.md"), "# API Reference").unwrap();

    let (_, content) = created.read_bundled_resource("references/api.md").unwrap();
    assert_eq!(content, "# API Reference");
    assert!(created.read_bundled_resource("../minikit.json").is_none());
}

#[test]
fn test_vault_mmap_bincode_index_lifecycle_and_invalidation() {
    let dir = tempdir().unwrap();
    let ws = dir.path();
    let global_vault = ws.join("global_vault");
    let store = VaultStore::new(ws).with_custom_global_vault(global_vault);

    // 1. Creating a project skill populates .minicode/vault/.vault_index.bin
    let created = store
        .create_skill(
            SkillScope::Project,
            "mmap-indexed-skill",
            "Zero-copy mmap indexed skill",
            Some("backend"),
            Some("reference"),
            "Initial instructions v1.",
            vec!["mmap".to_string()],
            vec![],
        )
        .unwrap();
    let skill_path_key = created.path.as_ref().unwrap().to_string_lossy().to_string();
    assert!(store.project_vault_index_file().exists());

    let idx = load_mmap_vault_index(&store.project_vault_index_file());
    assert_eq!(idx.magic, VAULT_INDEX_MAGIC);
    assert_eq!(idx.version, VAULT_INDEX_VERSION);
    let cached = idx.entries.get(&skill_path_key).unwrap();
    assert_eq!(cached.frontmatter.name, "mmap-indexed-skill");
    assert_eq!(cached.instructions, "Initial instructions v1.");

    // 2. Patching skill updates .vault_index.bin immediately
    store
        .update_skill(
            SkillScope::Project,
            "mmap-indexed-skill",
            "Updated instructions v2 via CRUD.",
        )
        .unwrap();
    let idx_updated = load_mmap_vault_index(&store.project_vault_index_file());
    assert_eq!(
        idx_updated
            .entries
            .get(&skill_path_key)
            .unwrap()
            .instructions,
        "Updated instructions v2 via CRUD."
    );

    // 3. Direct external edit to the .md file (changing file_size) invalidates and refreshes .vault_index.bin
    let raw_external = "---\nname: mmap-indexed-skill\ndescription: External edit\nkind: reference\n---\n\nExternal manual edit v3 with extra bytes!\n";
    fs::write(created.path.as_ref().unwrap(), raw_external).unwrap();

    let reloaded = store.get_skill("mmap-indexed-skill").unwrap();
    assert_eq!(reloaded.description, "External edit");
    assert!(reloaded
        .instructions
        .contains("External manual edit v3 with extra bytes!"));

    let idx_external = load_mmap_vault_index(&store.project_vault_index_file());
    assert_eq!(
        idx_external
            .entries
            .get(&skill_path_key)
            .unwrap()
            .frontmatter
            .description,
        "External edit"
    );

    // 4. Unloading skill prunes stale entry from .vault_index.bin on next scan
    store.unload_from_project("mmap-indexed-skill").unwrap();
    let _ = store.list_all_skills();
    let idx_pruned = load_mmap_vault_index(&store.project_vault_index_file());
    assert!(!idx_pruned.entries.contains_key(&skill_path_key));
}

#[test]
fn test_morphological_skill_matching_and_large_skill_turn_activation() {
    assert_eq!(VaultSkill::normalize_stem("debugging"), "debug");
    assert_eq!(VaultSkill::normalize_stem("refactoring"), "refactor");
    assert_eq!(VaultSkill::normalize_stem("planning"), "plan");

    let dir = tempdir().unwrap();
    let ws = dir.path();
    let store = VaultStore::new(ws);

    // Morphological inflection ("debugging" matches trigger "debug")
    let active = store.resolve_turn_active_skills::<&str>(
        "We are debugging a race condition in the auth service",
        None,
        &[],
    );
    assert!(active.contains(&"systematic-debugging".to_string()));

    // Large active skill (>4500 chars, e.g. frontend-design) summarized in static prompt
    // must still be dynamically injected in full when the turn prompt matches its triggers!
    store.load_to_project("frontend-design").unwrap();
    let dyn_skills = store.format_dynamic_turn_skills::<&str>(
        Some("Design a landing page hero component with modern typography"),
        None,
        &[],
        &[],
    );
    assert!(dyn_skills.is_some());
    let block = dyn_skills.unwrap();
    assert!(block.contains("Skill `frontend-design`"));
}
