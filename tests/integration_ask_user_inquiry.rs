use minicode::agent::inquiry::*;
use minicode::constants::TOTAL_TOOL_COUNT;
use minicode::tools::ToolRegistry;

#[test]
fn test_ask_user_invariants_and_schema_sync() {
    let schemas = ToolRegistry::get_tool_schemas();
    assert_eq!(
        schemas.len(),
        TOTAL_TOOL_COUNT,
        "TOTAL_TOOL_COUNT must match registry schema count exactly"
    );

    let ask_schema = schemas.iter().find(|s| s.name == "ask_user");
    assert!(
        ask_schema.is_some(),
        "ask_user must be present in registry schemas"
    );
}

#[test]
fn test_ask_user_auto_resolution_in_headless_mode() {
    let req = InquiryRequest {
        inquiry_id: "inq-head-1".to_string(),
        title: "Database Configuration".to_string(),
        description: Some("Pick your DB".to_string()),
        questions: vec![InquiryQuestion {
            id: "db_driver".to_string(),
            question: "Choose SQL engine".to_string(),
            header: Some("Database".to_string()),
            input_type: InquiryInputType::Choice,
            is_multi_select: false,
            allow_custom: false,
            placeholder: None,
            default_value: None,
            options: vec![
                InquiryOption {
                    id: "pg".to_string(),
                    label: "PostgreSQL 16".to_string(),
                    description: None,
                    recommended: true,
                },
                InquiryOption {
                    id: "sqlite".to_string(),
                    label: "SQLite 3".to_string(),
                    description: None,
                    recommended: false,
                },
            ],
        }],
    };

    let resp = req.auto_resolve_defaults();
    assert_eq!(resp.inquiry_id, "inq-head-1");
    assert!(!resp.cancelled);
    assert_eq!(resp.answers[0].selected_options, vec!["pg".to_string()]);
    let output = resp.into_tool_output();
    assert!(output.contains("\"status\":\"answered\""));
    assert!(output.contains("\"db_driver\":\"pg\""));
}

#[tokio::test]
async fn test_inquiry_interactive_suspension_channel() {
    let registry: InquiryRegistry =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let (tx, rx) = tokio::sync::oneshot::channel::<InquiryResponse>();

    registry.lock().unwrap().insert("call_99".to_string(), tx);

    let sender = registry
        .lock()
        .unwrap()
        .remove("call_99")
        .expect("Sender must exist");
    let send_res = sender.send(InquiryResponse {
        inquiry_id: "call_99".to_string(),
        answers: vec![InquiryAnswer {
            question_id: "auth".to_string(),
            selected_options: vec!["jwt".to_string()],
            custom_text: None,
            masked: false,
        }],
        cancelled: false,
    });
    assert!(send_res.is_ok());

    let received = rx.await.expect("Must receive from channel");
    assert_eq!(received.inquiry_id, "call_99");
    assert_eq!(
        received.answers[0].selected_options,
        vec!["jwt".to_string()]
    );
}

#[test]
fn test_inquiry_masked_secrets() {
    let ans = InquiryAnswer {
        question_id: "secret_token".to_string(),
        selected_options: vec![],
        custom_text: Some("ghp_1234567890abcdef".to_string()),
        masked: true,
    };
    assert_eq!(ans.display_value(), "••••••••");
    assert_eq!(ans.raw_value(), "ghp_1234567890abcdef");
}
