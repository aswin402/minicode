/// Integration tests for Phase 46: Native onpkg Tool Suite & Stack Scaffolder
///
/// Tests onpkg schema registration, stack info deserialization, and binary resolution.
use minicode::tools::onpkg::client::OnpkgClient;
use minicode::tools::onpkg::types::{OnpkgSkillInfo, OnpkgStackInfo};
use minicode::tools::registry::onpkg_tools;
use minicode::tools::ToolRegistry;

#[test]
fn test_onpkg_schemas_registered_in_registry() {
    let schemas = onpkg_tools::get_schemas();
    let names: Vec<String> = schemas.into_iter().map(|s| s.name).collect();

    assert!(names.contains(&"onpkg_stack_list".to_string()));
    assert!(names.contains(&"onpkg_stack_show".to_string()));
    assert!(names.contains(&"onpkg_stack_add".to_string()));
    assert!(names.contains(&"onpkg_skill_list".to_string()));
    assert!(names.contains(&"onpkg_skill_install".to_string()));
    assert!(names.contains(&"onpkg_sync".to_string()));
    assert!(names.contains(&"onpkg_doctor".to_string()));

    // Verify presence in global ToolRegistry
    let global_schemas = ToolRegistry::get_tool_schemas();
    let global_names: Vec<String> = global_schemas.into_iter().map(|s| s.name).collect();
    assert!(global_names.contains(&"onpkg_stack_list".to_string()));
    assert!(global_names.contains(&"onpkg_sync".to_string()));
}

#[test]
fn test_onpkg_stack_info_deserialization() {
    let raw_json = r#"[
        {
            "name": "next-template",
            "category": "frontend",
            "description": "Upgraded Next.js 16 + Bun + Tailwind CSS v4 + Prisma 7",
            "version": "1.0.0",
            "files_count": 61,
            "technologies": ["next", "prisma", "tailwind"]
        },
        {
            "name": "fastapi",
            "category": "backend",
            "description": "FastAPI + SQLAlchemy (Async) + Alembic + Pydantic v2",
            "version": "1.0.0",
            "files_count": 24,
            "technologies": ["fastapi", "python"]
        }
    ]"#;

    let stacks: Vec<OnpkgStackInfo> = serde_json::from_str(raw_json).unwrap();
    assert_eq!(stacks.len(), 2);
    assert_eq!(stacks[0].name, "next-template");
    assert_eq!(stacks[0].files_count, 61);
    assert_eq!(stacks[0].technologies.len(), 3);
    assert_eq!(stacks[1].category, "backend");
}

#[test]
fn test_onpkg_skill_info_deserialization() {
    let skill = OnpkgSkillInfo {
        name: "tailwind-patterns".to_string(),
        version: "1.0.0".to_string(),
        description: "Tailwind CSS v4 design tokens and modern patterns".to_string(),
    };

    let serialized = serde_json::to_string(&skill).unwrap();
    let deserialized: OnpkgSkillInfo = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.name, "tailwind-patterns");
}

#[test]
fn test_onpkg_client_binary_check() {
    // Should gracefully execute without panicking
    let _ = OnpkgClient::find_binary();
    let _ = OnpkgClient::is_installed();
}
