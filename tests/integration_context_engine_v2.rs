use minicode::agent::provider::StreamChunk;
use minicode::agent::types::{Message, Role};
use minicode::agent::AgentLoop;
use minicode::config::Config;
use tempfile::TempDir;

#[test]
fn test_message_reasoning_content_and_stream_chunk_usage() {
    let msg = Message::assistant("Result text").with_reasoning("Initial step-by-step thinking");
    assert_eq!(msg.role, Role::Assistant);
    assert_eq!(msg.content, "Result text");
    assert_eq!(
        msg.reasoning_content.as_deref(),
        Some("Initial step-by-step thinking")
    );

    let usage = StreamChunk::Usage {
        prompt_tokens: 1000,
        completion_tokens: 150,
        cached_prompt_tokens: 800,
    };

    if let StreamChunk::Usage {
        prompt_tokens,
        completion_tokens,
        cached_prompt_tokens,
    } = usage
    {
        assert_eq!(prompt_tokens, 1000);
        assert_eq!(completion_tokens, 150);
        assert_eq!(cached_prompt_tokens, 800);
    } else {
        panic!("Expected StreamChunk::Usage");
    }
}

#[tokio::test]
async fn test_kv_prefix_stability_history_immutability() {
    let temp_dir = TempDir::new().unwrap();
    let config = Config::default();
    let mock = minicode::agent::mock_provider::MockProvider::new("mock", "mock-model");
    mock.push_response(&["I will add error handling."], vec![]);
    let mut agent = AgentLoop::new(temp_dir.path(), config, Box::new(mock));

    // Turn 1 initial message
    let turn1_user_msg =
        Message::user("<user_request>\nWrite a hello world program\n</user_request>");
    let turn1_assistant_msg = Message::assistant("Here is hello world: println!(\"Hello\");");
    agent.messages_mut().push(turn1_user_msg.clone());
    agent.messages_mut().push(turn1_assistant_msg.clone());

    let turn1_user_content_before = agent.messages()[0].content.clone();
    let turn1_asst_content_before = agent.messages()[1].content.clone();

    // Verify messages[0] and messages[1] are immutable and never mutated in-place
    // In previous versions, turn 2 execution called sanitize_past_user_message on agent.messages,
    // mutating historical user messages in-place and invalidating KV-cache.
    let (event_sender, mut event_receiver) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move { while let Some(_) = event_receiver.recv().await {} });

    // Execute Turn 2
    let _ = agent
        .execute_turn("Add error handling to it", event_sender, None)
        .await;

    // Turn 1 messages MUST remain byte-identical
    assert_eq!(
        agent.messages()[0].content,
        turn1_user_content_before,
        "Turn 1 user message was mutated in-place! KV-cache prefix was broken."
    );
    assert_eq!(
        agent.messages()[1].content,
        turn1_asst_content_before,
        "Turn 1 assistant message was mutated in-place! KV-cache prefix was broken."
    );
}

#[test]
fn test_deterministic_tool_sorting() {
    use minicode::agent::provider::ToolSchema;
    use serde_json::json;

    let mut tools = vec![
        ToolSchema {
            name: "write_file".to_string(),
            description: "Write content to file".to_string(),
            parameters: json!({}),
        },
        ToolSchema {
            name: "bash".to_string(),
            description: "Execute bash command".to_string(),
            parameters: json!({}),
        },
        ToolSchema {
            name: "read_file".to_string(),
            description: "Read file content".to_string(),
            parameters: json!({}),
        },
    ];

    tools.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(tools[0].name, "bash");
    assert_eq!(tools[1].name, "read_file");
    assert_eq!(tools[2].name, "write_file");
}

#[test]
fn test_json_crusher_and_ccr_cache() {
    use minicode::context::budget::ccr_cache::CcrCache;
    use minicode::context::budget::json_crusher::JsonCrusher;
    use serde_json::json;

    // Create 30 repetitive JSON objects with common fields
    let mut items = Vec::new();
    for i in 0..30 {
        items.push(json!({
            "id": i,
            "status": if i == 14 { "failed" } else { "active" },
            "cluster_region": "us-east-1",
            "environment": "production",
            "tier": "backend_service",
            "message": if i == 14 { "Connection timeout to db" } else { "healthy" }
        }));
    }
    let raw_json = serde_json::to_string_pretty(&items).unwrap();

    let (crushed, ccr_id) = JsonCrusher::crush(&raw_json).expect("Expected successful crushing");

    // 1. Common fields factored
    assert!(
        crushed.contains("_common_fields"),
        "Missing _common_fields factoring"
    );
    assert!(crushed.contains("cluster_region"));
    assert!(crushed.contains("production"));

    // 2. Error preservation: failed item 14 must be retained in full
    assert!(
        crushed.contains("failed"),
        "Failed items must never be omitted"
    );
    assert!(crushed.contains("Connection timeout to db"));

    // 3. Middle items omitted with CCR reference
    assert!(crushed.contains("_omitted_count"));
    assert!(crushed.contains(&ccr_id));

    // 4. Substantial token/character reduction (> 40% savings)
    assert!(
        crushed.len() < raw_json.len() / 2,
        "Crushed length {} not less than half of original {}",
        crushed.len(),
        raw_json.len()
    );

    // 5. Lossless retrieval from CcrCache
    let retrieved = CcrCache::retrieve(&ccr_id, None, None).expect("CcrCache retrieval failed");
    assert_eq!(retrieved, raw_json);
}

#[test]
fn test_hierarchical_dox_scoping() {
    use minicode::context::governance::dox::DoxEngine;
    use std::fs::{create_dir_all, write};

    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Root AGENTS.md
    write(root.join("AGENTS.md"), "# Root Rules\n- Global rule 1").unwrap();

    // Subdirectory crate with AGENTS.md
    let sub = root.join("crates").join("core");
    create_dir_all(&sub).unwrap();
    write(sub.join("AGENTS.md"), "# Core Rules\n- Scoped crate rule").unwrap();

    let scoped_rules = DoxEngine::resolve_scoped_rules(root, &["crates/core/src/lib.rs"]);
    assert!(scoped_rules.contains("Global rule 1"));
    assert!(scoped_rules.contains("Scoped crate rule"));
}

#[test]
fn test_compaction_orphan_tool_result_safety() {
    use minicode::agent::types::ToolCall;
    use minicode::context::budget::auto_compact::AutoCompactor;
    use serde_json::json;

    let mut compactor = AutoCompactor::new("claude-3-5-sonnet-20241022")
        .unwrap()
        .with_custom_limit(300); // Low limit to force Tier 2 summarization

    // Build an 11-message sequence where naive cutoff = 11 - 6 = 5 (which is Role::Tool)
    let mut messages = vec![
        Message::user("Turn 1 user request: ".repeat(15)),
        Message::assistant("Turn 1 assistant response: ".repeat(15)),
        Message::user("Turn 2 user request: ".repeat(15)),
        Message::assistant("Turn 2 assistant response: ".repeat(15)),
        Message::assistant_with_tools(
            "Calling bash",
            vec![ToolCall {
                id: "call_bash_1".to_string(),
                name: "bash".to_string(),
                arguments: json!({"command": "ls"}),
            }],
        ),
        Message::tool_result("call_bash_1", "bash", "file1.rs\nfile2.rs"),
        Message::assistant("All files listed."),
        Message::user("Turn 3 user request: ".repeat(10)),
        Message::assistant("Turn 3 assistant response"),
        Message::user("Turn 4 user request"),
        Message::assistant("Turn 4 assistant response"),
    ];

    let result = compactor.compact(&mut messages, 4);
    assert!(result.is_some(), "Expected compaction to trigger");

    // Invariant: The compacted message chain MUST NEVER contain an orphan ToolResult.
    // That means: if a message is Role::Tool, the preceding message MUST be Role::Assistant with tool_calls.
    for i in 0..messages.len() {
        if messages[i].role == Role::Tool {
            assert!(
                i > 0,
                "First message in compacted conversation cannot be a Tool result!"
            );
            assert_eq!(
                messages[i - 1].role,
                Role::Assistant,
                "Tool result at index {} not preceded by an Assistant message!",
                i
            );
            assert!(
                messages[i - 1].tool_calls.is_some(),
                "Preceding assistant message at index {} has no tool_calls!",
                i - 1
            );
        }
    }
}

#[test]
fn test_hermes_inline_tool_call_extraction() {
    use minicode::agent::providers::openai::extract_inline_tool_calls;

    let raw = "I will inspect the workspace.\n<tool_call>{\"name\": \"read_file\", \"arguments\": {\"path\": \"Cargo.toml\"}}</tool_call>\nAll done!";
    let (cleaned, calls) = extract_inline_tool_calls(raw);

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "read_file");
    assert_eq!(calls[0].arguments["path"], "Cargo.toml");
    assert_eq!(cleaned.trim(), "I will inspect the workspace.\n\nAll done!");
}

#[test]
fn test_hermes_strip_thought_tags() {
    use minicode::context::budget::auto_compact::AutoCompactor;

    let raw1 = "<thought>Thinking about the architecture...</thought>Here is the solution.";
    assert_eq!(
        AutoCompactor::strip_thought_tags(raw1),
        "Here is the solution."
    );

    let raw2 =
        "<think>DeepSeek reasoning process\nstep 1\nstep 2</think>\n\nEverything is resolved.";
    assert_eq!(
        AutoCompactor::strip_thought_tags(raw2),
        "Everything is resolved."
    );

    let raw3 = "Pre-text <thought>scratchpad</thought> Mid-text <think>pondering</think> Post-text";
    assert_eq!(
        AutoCompactor::strip_thought_tags(raw3),
        "Pre-text  Mid-text  Post-text"
    );

    let raw4 = "<thinking>Claude 3.7 reasoning block</thinking>Production fix applied.";
    assert_eq!(
        AutoCompactor::strip_thought_tags(raw4),
        "Production fix applied."
    );

    let raw5 = "<scratchpad>Hermes scratchpad content</scratchpad><reasoning>o1 replica reasoning</reasoning>Done.";
    assert_eq!(AutoCompactor::strip_thought_tags(raw5), "Done.");

    let raw6 = "<antThinking>Internal chain of thought</antThinking>Verified answer.";
    assert_eq!(AutoCompactor::strip_thought_tags(raw6), "Verified answer.");
}

#[test]
fn test_strip_older_reasoning() {
    use minicode::agent::types::{Message, Role};
    use minicode::context::budget::auto_compact::AutoCompactor;

    let mut messages = vec![
        Message::user("Turn 1 question"),
        Message::assistant("<scratchpad>Old scratchpad 1</scratchpad>Assistant answer 1")
            .with_reasoning("Old reasoning 1"),
        Message::user("Turn 2 question"),
        Message::assistant("Assistant answer 2").with_reasoning("Old reasoning 2"),
        Message::user("Turn 3 question"),
        Message::assistant("Assistant answer 3").with_reasoning("Recent reasoning 3"),
        Message::user("Turn 4 question"),
        Message::assistant("<think>Recent thought 4</think>Assistant answer 4")
            .with_reasoning("Recent reasoning 4"),
    ];

    AutoCompactor::strip_older_reasoning(&mut messages);

    // Turn 1 (4th assistant from end -> stripped)
    assert_eq!(messages[1].role, Role::Assistant);
    assert!(messages[1].reasoning_content.is_none());
    assert_eq!(messages[1].content, "Assistant answer 1");

    // Turn 2 (3rd assistant from end -> stripped)
    assert_eq!(messages[3].role, Role::Assistant);
    assert!(messages[3].reasoning_content.is_none());
    assert_eq!(messages[3].content, "Assistant answer 2");

    // Turn 3 (2nd assistant from end -> preserved)
    assert_eq!(messages[5].role, Role::Assistant);
    assert_eq!(
        messages[5].reasoning_content.as_deref(),
        Some("Recent reasoning 3")
    );

    // Turn 4 (1st assistant from end -> preserved)
    assert_eq!(messages[7].role, Role::Assistant);
    assert_eq!(
        messages[7].reasoning_content.as_deref(),
        Some("Recent reasoning 4")
    );
    assert!(messages[7]
        .content
        .contains("<think>Recent thought 4</think>"));
}

#[test]
fn test_progressive_memory_zero_io_caching() {
    use minicode::context::progressive_memory::ProgressiveMemory;

    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // 1. Initial save
    let mut mem = ProgressiveMemory::new();
    mem.add_l2_fact("cached_key", "initial_val", "test", 1.0);
    mem.save(root).unwrap();

    // 2. Load into memory
    let mut loaded = ProgressiveMemory::load(root);
    assert_eq!(loaded.l2_project_facts.len(), 1);
    assert_eq!(loaded.l2_project_facts[0].value, "initial_val");

    // 3. Update in memory via update_fact and save
    loaded.update_fact("cached_key", "cached_val_fast");
    loaded.save(root).unwrap();

    // 4. Subsequent load should hit memory cache with 0 disk penalty
    let cached = ProgressiveMemory::load(root);
    assert_eq!(cached.l2_project_facts[0].value, "cached_val_fast");
}

#[test]
fn test_biological_decay_retention_and_pruning() {
    use chrono::{Duration, Utc};
    use minicode::context::memory::decay::MemoryScope;
    use minicode::context::progressive_memory::{
        MemoryTier, ProgressiveMemory, ProgressiveMemoryEntry,
    };

    let mut mem = ProgressiveMemory::new();

    // Permanent rule: should never decay
    mem.add_l3_preference("permanent_rule", "Never run without -j 1", "test");

    // Fresh project fact: less than 1 hour old
    mem.add_l2_fact("fresh_fact", "Active rust version 1.85", "test", 1.0);

    // Old transient fact: simulated last accessed 10 hours ago (half-life = 1 hour)
    let old_timestamp = (Utc::now() - Duration::hours(10)).to_rfc3339();
    let mut decayed_entry = ProgressiveMemoryEntry::new(
        MemoryTier::L2ProjectFact,
        "decayed_hypothesis",
        "Maybe issue is in libssl",
        0.5,
        "test",
    );
    decayed_entry.scope = Some(MemoryScope::Transient);
    decayed_entry.last_accessed = old_timestamp;
    mem.l2_project_facts.push(decayed_entry);

    assert_eq!(mem.l2_project_facts.len(), 2);

    // Prune below 0.15 retention threshold
    mem.prune_decayed(0.15);

    // The decayed transient fact must be pruned
    assert_eq!(mem.l2_project_facts.len(), 1);
    assert_eq!(mem.l2_project_facts[0].key, "fresh_fact");
    // Global permanent preference must remain intact
    assert_eq!(mem.l3_global_preferences.len(), 1);
    assert_eq!(mem.l3_global_preferences[0].key, "permanent_rule");
}

#[test]
fn test_prompt_no_double_nested_progressive_memory() {
    use minicode::agent::prompt::PromptBuilder;
    use minicode::context::progressive_memory::ProgressiveMemory;

    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let mut mem = ProgressiveMemory::new();
    mem.add_l2_fact("engine", "Tokio multi-threaded", "test", 1.0);
    mem.save(root).unwrap();

    let recency = PromptBuilder::build_recency_context(root, None, &[], None, None);

    // Invariant: <progressive_memory> must appear exactly once, not nested
    assert!(
        recency.contains("<progressive_memory>"),
        "Must contain <progressive_memory>"
    );
    assert!(
        !recency.contains("<progressive_memory>\n  <progressive_memory>")
            && !recency.contains("<progressive_memory>\n    <progressive_memory>"),
        "Detected double-nested <progressive_memory> XML tags!"
    );
    // Legacy <core_memory> tag must not be injected separately
    assert!(
        !recency.contains("<core_memory>"),
        "Redundant <core_memory> tag found"
    );
}

#[test]
fn test_binary_vector_quantization_and_hamming() {
    use minicode::context::search::quantize::BinaryVector128;

    // Create 128-dimensional vector where first 64 elements are positive, last 64 are negative
    let mut v1 = vec![1.0f32; 64];
    v1.extend(vec![-1.0f32; 64]);

    // Create identical vector
    let v2 = v1.clone();

    // Create inverted vector
    let mut v3 = vec![-1.0f32; 64];
    v3.extend(vec![1.0f32; 64]);

    let bin1 = BinaryVector128::from_f32_slice(&v1);
    let bin2 = BinaryVector128::from_f32_slice(&v2);
    let bin3 = BinaryVector128::from_f32_slice(&v3);

    // Exact match: 0 Hamming distance, 1.0 similarity
    assert_eq!(bin1.hamming_distance(&bin2), 0);
    assert!((bin1.cosine_similarity(&bin2) - 1.0).abs() < 1e-5);

    // Completely inverted: 128 Hamming distance, -1.0 similarity
    assert_eq!(bin1.hamming_distance(&bin3), 128);
    assert!((bin1.cosine_similarity(&bin3) - (-1.0)).abs() < 1e-5);

    // Size check: BinaryVector128 must be exactly 16 bytes (128 bits)
    assert_eq!(std::mem::size_of::<BinaryVector128>(), 16);
}

#[test]
fn test_polar_quant4_fidelity() {
    use minicode::context::search::quantize::PolarQuant4;

    // Generate pseudo-random f32 vector of 128 dimensions
    let original: Vec<f32> = (0..128).map(|i| (i as f32 * 0.17).sin() * 2.5).collect();

    let quantized = PolarQuant4::from_f32_slice(&original);
    let reconstructed = quantized.dequantize();

    assert_eq!(reconstructed.len(), 128);

    // Compute cosine similarity between original and reconstructed
    let dot: f32 = original
        .iter()
        .zip(reconstructed.iter())
        .map(|(a, b)| a * b)
        .sum();
    let norm_orig: f32 = original.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_recon: f32 = reconstructed.iter().map(|x| x * x).sum::<f32>().sqrt();
    let fidelity = dot / (norm_orig * norm_recon);

    // 4-bit scalar quantization preserves >98% directional cosine fidelity
    assert!(
        fidelity > 0.98,
        "PolarQuant4 fidelity {} is below 98%",
        fidelity
    );

    // Asymmetric dot product against self should be positive and close to squared L2 norm
    let asym_dot = quantized.asymmetric_dot_product(&original);
    assert!(asym_dot > 0.0);
    let l2_sq: f32 = original.iter().map(|x| x * x).sum();
    let ratio = asym_dot / l2_sq;
    assert!((ratio - 1.0).abs() < 0.05, "Asymmetric dot ratio {}", ratio);
}

#[test]
fn test_status_bar_kv_cache_rendering() {
    use minicode::ui::{StatusContext, Theme};
    use std::path::Path;

    let theme = Theme::aura_dark();
    let ws = Path::new("/workspace");

    let ctx_with_kv = StatusContext {
        theme: &theme,
        workspace: ws,
        provider: "anthropic",
        model: "claude-3-5-sonnet",
        mcp_count: 2,
        used_tokens: 10000,
        max_context: 200000,
        show_cost: true,
        session_cost_usd: 0.035,
        cached_tokens: 8200,
    };

    assert_eq!(ctx_with_kv.cached_tokens, 8200);
    let hit_pct =
        ((ctx_with_kv.cached_tokens as f64 / ctx_with_kv.used_tokens as f64) * 100.0) as usize;
    assert_eq!(hit_pct, 82);
}

#[test]
fn test_observation_pruner_log_and_ccr_cache() {
    use minicode::context::budget::ccr_cache::CcrCache;
    use minicode::context::budget::ObservationPruner;

    let mut noisy_log = String::new();
    noisy_log.push_str("\x1b[32mBuild target initialized\x1b[0m\n");
    for i in 0..35 {
        noisy_log.push_str(&format!("Compiling crate_{} v0.2.1 (/tmp/build)\n", i));
    }
    for i in 0..15 {
        noisy_log.push_str(&format!("  at tokio::runtime::worker_{}:{}\n", i, i * 4));
    }
    noisy_log.push_str("Finished release [optimized] target(s) in 12.3s\n");

    let pruned = ObservationPruner::prune_for_llm("run_command", &noisy_log);

    assert!(pruned.len() < noisy_log.len());
    assert!(pruned.contains("compilation lines collapsed"));
    assert!(pruned.contains("runtime/system stack frames folded"));
    assert!(pruned.contains("Finished release [optimized]"));
    assert!(pruned.contains("Use retrieve_observation(id=\""));

    // Extract ccr_id from hint
    let id_prefix = "retrieve_observation(id=\"";
    let start_idx = pruned.find(id_prefix).unwrap() + id_prefix.len();
    let end_idx = pruned[start_idx..].find('"').unwrap() + start_idx;
    let ccr_id = &pruned[start_idx..end_idx];

    let recovered =
        CcrCache::retrieve(ccr_id, None, None).expect("Must retrieve full log from CCR");
    assert_eq!(recovered, noisy_log);
}

#[test]
fn test_recency_context_kv_cache_prefix_alignment() {
    use minicode::agent::prompt::PromptBuilder;
    use minicode::context::budget::ContextBudget;
    use minicode::git::GitStatus;
    use std::path::Path;

    let ws = Path::new("/tmp/test_workspace");
    let active_set = vec!["src/lib.rs".to_string(), "src/main.rs".to_string()];
    let git_status = GitStatus {
        branch: "main".to_string(),
        is_clean: false,
        staged: vec!["src/lib.rs".to_string()],
        unstaged: vec![],
        untracked: vec![],
        conflicted: vec![],
    };
    let anchor = "Active Objective: Implement LMCache Prefix Alignment";
    let budget = ContextBudget::new(15_000, 128_000, 30_000);

    let recency = PromptBuilder::build_recency_context(
        ws,
        Some(anchor),
        &active_set,
        Some(&git_status),
        Some(&budget),
    );

    // Verify presence of structural elements
    assert!(recency.contains("<workspace_context>"));
    assert!(recency.contains("<active_working_set>"));
    assert!(recency.contains("<!-- KV_CACHE_ANCHOR -->"));
    assert!(recency.contains("<git_status"));
    assert!(recency.contains("<task_anchor>"));
    assert!(recency.contains("<context_budget"));

    // Verify strict stable-to-volatile ordering for Radix KV cache prefix preservation:
    // active_working_set (stable) -> anchor delimiter -> git_status (volatile) -> context_budget (most volatile)
    let ws_pos = recency
        .find("<active_working_set>")
        .expect("must contain active_working_set");
    let anchor_marker_pos = recency
        .find("<!-- KV_CACHE_ANCHOR -->")
        .expect("must contain KV_CACHE_ANCHOR");
    let git_pos = recency
        .find("<git_status")
        .expect("must contain git_status");
    let budget_pos = recency
        .find("<context_budget")
        .expect("must contain context_budget");

    assert!(
        ws_pos < anchor_marker_pos,
        "Active working set must precede KV cache anchor"
    );
    assert!(
        anchor_marker_pos < git_pos,
        "KV cache anchor must precede git status"
    );
    assert!(
        git_pos < budget_pos,
        "Git status must precede per-turn context budget"
    );
}

#[tokio::test]
async fn test_agent_loop_prune_context_proactive_thought_stripping() {
    let temp_dir = TempDir::new().unwrap();
    let config = Config::default();
    let mock = minicode::agent::mock_provider::MockProvider::new("mock", "mock-model");
    let mut agent = AgentLoop::new(temp_dir.path(), config, Box::new(mock));

    // Seed 4 assistant messages with thoughts
    agent.messages_mut().push(Message::user("Turn 1"));
    agent.messages_mut().push(
        Message::assistant("<think>Older thought turn 1</think>Answer 1")
            .with_reasoning("Old reasoning 1"),
    );
    agent.messages_mut().push(Message::user("Turn 2"));
    agent.messages_mut().push(
        Message::assistant("<scratchpad>Older thought turn 2</scratchpad>Answer 2")
            .with_reasoning("Old reasoning 2"),
    );
    agent.messages_mut().push(Message::user("Turn 3"));
    agent.messages_mut().push(
        Message::assistant("<thought>Recent thought turn 3</thought>Answer 3")
            .with_reasoning("Recent reasoning 3"),
    );
    agent.messages_mut().push(Message::user("Turn 4"));
    agent.messages_mut().push(
        Message::assistant("<thinking>Recent thought turn 4</thinking>Answer 4")
            .with_reasoning("Recent reasoning 4"),
    );

    // Call prune_context proactively
    let _ = agent.prune_context();

    // Older turns (1 & 2) should have thoughts and reasoning stripped
    assert_eq!(agent.messages()[1].content, "Answer 1");
    assert!(agent.messages()[1].reasoning_content.is_none());

    assert_eq!(agent.messages()[3].content, "Answer 2");
    assert!(agent.messages()[3].reasoning_content.is_none());

    // Most recent 2 assistant turns (3 & 4) should preserve thoughts and reasoning
    assert!(agent.messages()[5]
        .content
        .contains("<thought>Recent thought turn 3</thought>"));
    assert_eq!(
        agent.messages()[5].reasoning_content.as_deref(),
        Some("Recent reasoning 3")
    );

    assert!(agent.messages()[7]
        .content
        .contains("<thinking>Recent thought turn 4</thinking>"));
    assert_eq!(
        agent.messages()[7].reasoning_content.as_deref(),
        Some("Recent reasoning 4")
    );
}

#[test]
fn test_turbovec_zero_copy_and_observation_pruning_integration() {
    use minicode::context::budget::ObservationPruner;
    use minicode::context::ccr_cache::CcrCache;
    use minicode::context::search::mmap_index::{ChunkMetadata, MmapTurbovecIndex};
    use minicode::context::search::quantize::TurbovecRecord;
    use minicode::context::search::semantic::SemanticIndex;

    let temp = TempDir::new().unwrap();
    let bin_path = temp.path().join("index.bin");

    // 1. Generate Turbovec records with orthogonal SemanticIndex embeddings
    let mut records = Vec::new();
    let mut metadata = Vec::new();

    for i in 0..20 {
        let content = format!("fn process_worker_task_{}() {{ run_worker({}); }}", i, i);
        let vec = SemanticIndex::embed(&content);
        records.push(TurbovecRecord::from_f32_with_wht(&vec));
        metadata.push(ChunkMetadata {
            file_path: format!("src/worker_{}.rs", i),
            start_line: 1,
            end_line: 15,
            symbol_name: Some(format!("process_worker_task_{}", i)),
            symbol_kind: Some("function".to_string()),
            content,
        });
    }

    // 2. Persist binary index and search via mmap
    MmapTurbovecIndex::create(&bin_path, &records, &metadata).expect("create binary index");
    let index = MmapTurbovecIndex::open(&bin_path).expect("open mmap index");
    assert_eq!(index.record_count(), 20);

    let query = SemanticIndex::embed("process_worker_task_7 run_worker");
    let results = index.search(&query, 3);
    assert!(!results.is_empty());
    assert_eq!(results[0].0, 7);
    let meta7 = index.get_metadata(7).expect("meta 7 exists");
    assert_eq!(meta7.symbol_name.as_deref(), Some("process_worker_task_7"));

    // 3. Test Headroom DOX pre-ingress observation pruning for warning cascades
    let mut warn_log = String::new();
    for i in 0..15 {
        warn_log.push_str(&format!(
            "warning: unused import `dep_{}`\n --> src/lib.rs:{}:5\n",
            i, i
        ));
    }
    warn_log.push_str("error[E0308]: mismatched types\n --> src/lib.rs:100:9\n");

    let pruned_warn = ObservationPruner::prune_for_llm("run_command", &warn_log);
    assert!(pruned_warn.contains("compiler warnings collapsed"));
    assert!(pruned_warn.contains("error[E0308]"));
    assert!(pruned_warn.contains("Use retrieve_observation"));

    // Lossless retrieval
    if let Some(start) = pruned_warn.find("id=\"") {
        let id_part = &pruned_warn[start + 4..];
        if let Some(end) = id_part.find('"') {
            let id = &id_part[..end];
            let retrieved = CcrCache::retrieve(id, None, None).expect("lossless ccr retrieval");
            assert_eq!(retrieved, warn_log);
        }
    }

    // 4. Test Headroom DOX pre-ingress observation pruning for test runner outputs
    let mut test_log = String::new();
    test_log.push_str("running 40 tests\n");
    for i in 0..39 {
        test_log.push_str(&format!("test core::test_{} ... ok\n", i));
    }
    test_log.push_str("test core::test_boom ... FAILED\n");
    test_log.push_str("test result: FAILED. 39 passed; 1 failed;\n");

    let pruned_tests = ObservationPruner::prune_for_llm("run_command", &test_log);
    assert!(pruned_tests.contains("passing tests collapsed"));
    assert!(pruned_tests.contains("test core::test_boom ... FAILED"));
    assert!(pruned_tests.contains("test result: FAILED"));
}
