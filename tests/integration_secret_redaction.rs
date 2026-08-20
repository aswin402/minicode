use minicode::constants::REDACTED_PLACEHOLDER;
use minicode::sandbox::redact::SecretRedactor;

/// Integration tests use the global redactor instance which is safe for
/// pattern-based redaction tests (env harvesting doesn't interfere because
/// we only test against known regex patterns that won't match normal env values).

#[test]
fn test_redactor_masks_openai_key_in_tool_output() {
    let r = SecretRedactor::global();
    let simulated_output =
        "Config loaded. OPENAI_API_KEY=sk-abc123def456ghi789jkl012mno345pqr678\nReady.";
    let result = r.redact(simulated_output);
    assert!(
        !result.contains("sk-abc123def456"),
        "OpenAI key must be masked in tool output"
    );
    assert!(result.contains(REDACTED_PLACEHOLDER));
    assert!(result.contains("Config loaded."));
    assert!(result.contains("Ready."));
}

#[test]
fn test_redactor_masks_github_token_in_git_output() {
    let r = SecretRedactor::global();
    let simulated_output = "remote: Invalid credentials for 'https://ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij1234@github.com/user/repo.git'";
    let result = r.redact(simulated_output);
    assert!(
        !result.contains("ghp_ABCDEF"),
        "GitHub PAT must be masked"
    );
}

#[test]
fn test_redactor_masks_private_key_in_file_read() {
    let r = SecretRedactor::global();
    let simulated_output = r#"Contents of id_rsa:
-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEA0Z3VS5JJcds3xfn/ygWyF8PbnGy0AHB7MhgHcTz6sE2I2yGP
base64encodedkeydata+more/data==
-----END RSA PRIVATE KEY-----
EOF"#;
    let result = r.redact(simulated_output);
    assert!(
        !result.contains("MIIEpAIBAAK"),
        "Private key content must be masked"
    );
    assert!(result.contains("[REDACTED: PRIVATE KEY]"));
    assert!(result.contains("Contents of id_rsa:"));
    assert!(result.contains("EOF"));
}

#[test]
fn test_redactor_masks_bearer_in_curl_output() {
    let r = SecretRedactor::global();
    let simulated_output = r#"> GET /api/data HTTP/2
> Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U
> Accept: application/json
< HTTP/2 200 OK"#;
    let result = r.redact(simulated_output);
    assert!(
        !result.contains("eyJhbGciOi"),
        "Bearer JWT must be masked"
    );
    assert!(result.contains("Bearer [REDACTED]"));
    assert!(result.contains("GET /api/data"));
    assert!(result.contains("HTTP/2 200 OK"));
}

#[test]
fn test_redactor_preserves_normal_cargo_output() {
    let r = SecretRedactor::global();
    let normal_output = r#"   Compiling minicode v0.0.36 (/home/user/minicode)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.34s
     Running unittests src/main.rs (target/debug/deps/minicode-abc123)

running 171 tests
test result: ok. 171 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.23s"#;
    let result = r.redact(normal_output);
    assert_eq!(
        result, normal_output,
        "Normal cargo output should not be modified"
    );
}

#[test]
fn test_redactor_masks_aws_access_key() {
    let r = SecretRedactor::global();
    let simulated_output = "aws_access_key_id = AKIAIOSFODNN7EXAMPLE";
    let result = r.redact(simulated_output);
    assert!(
        !result.contains("AKIAIOSFODNN7EXAMPLE"),
        "AWS access key must be masked"
    );
    assert!(result.contains(REDACTED_PLACEHOLDER));
}

#[test]
fn test_redactor_masks_multiple_secret_types() {
    let r = SecretRedactor::global();
    let simulated_output = "AWS key: AKIAIOSFODNN7EXAMPLE, OpenAI: sk-proj-abcdefghij1234567890abcdefghij, Stripe: rk_test_FAKE000000000000000000000000";
    let result = r.redact(simulated_output);
    assert!(
        !result.contains("AKIAIOSFODNN7EXAMPLE"),
        "AWS key must be masked"
    );
    assert!(
        !result.contains("sk-proj-abcdef"),
        "OpenAI project key must be masked"
    );
    assert!(
        !result.contains("rk_test_FAKE"),
        "Stripe key must be masked"
    );
}
