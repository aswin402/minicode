use minicode::context::skill_forge::SkillForge;
use minicode::tools::ToolRegistry;
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn test_skill_forge_create_list_inspect_lifecycle() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    // 1. Create a skill via SkillForge
    let create_msg = SkillForge::create_skill(
        ws,
        "security-audit",
        "Guidelines for scanning credentials and private keys",
        "1. Check .env.example\n2. Never commit secrets\n3. Use varlock for env management.",
        &["read_file".to_string(), "grep_search".to_string()],
    )
    .unwrap();

    assert!(create_msg.contains("security-audit"));

    // 2. List skills
    let list = SkillForge::list_all_skills(ws).unwrap();
    assert!(list.contains("security-audit"));
    assert!(list.contains("scanning credentials"));

    // 3. Inspect skill
    let skill = SkillForge::inspect_skill(ws, "security-audit").unwrap();
    assert_eq!(skill.name, "security-audit");
    assert!(skill.instructions.contains("Never commit secrets"));
}

#[tokio::test]
async fn test_skill_forge_tool_dispatch() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    // 1. Dispatch create_skill tool
    let args_create = json!({
        "name": "react-perf",
        "description": "React performance optimization strategies",
        "instructions": "Use React.memo and useMemo appropriately to prevent re-renders.",
        "allowed_tools": ["patch_file"]
    });

    let res_create =
        ToolRegistry::dispatch(ws, "call_1", "create_skill", &args_create, None, 1).await;
    assert!(res_create.success);
    assert!(res_create.output.contains("react-perf"));

    // 2. Dispatch list_skills tool
    let res_list = ToolRegistry::dispatch(ws, "call_2", "list_skills", &json!({}), None, 1).await;
    assert!(res_list.success);
    assert!(res_list.output.contains("react-perf"));

    // 3. Dispatch inspect_skill tool
    let args_inspect = json!({
        "name": "react-perf"
    });
    let res_inspect =
        ToolRegistry::dispatch(ws, "call_3", "inspect_skill", &args_inspect, None, 1).await;
    assert!(res_inspect.success);
    assert!(res_inspect.output.contains("React.memo"));
}
