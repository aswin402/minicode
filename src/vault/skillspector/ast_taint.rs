//! Tree-sitter CST/AST Call-Graph & Intraprocedural Dataflow Taint Engine (`petgraph::DiGraph`).
//!
//! Parses Python, JavaScript, TypeScript, and Rust scripts into Concrete Syntax Trees,
//! extracts variable bindings (`Def -> Use`), import aliases, attribute reads, keyword arguments, and call sites,
//! and runs graph reachability (`petgraph::algo::astar`) across a declarative `(SourceKind, SinkKind)` taint matrix.

use super::{ExercisedCapability, FindingCategory, FindingSeverity, SkillFinding};
use crate::constants::{SECRET_PATTERNS, SKILLSPECTOR_COMPACT_SNIPPET_LEN};
use petgraph::algo::astar;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap, HashSet};
use tree_sitter::{Language, Node, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    EnvSecret,
    CredentialFile,
    NetworkInput,
    CommandOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SinkKind {
    NetworkExfil,
    DynamicEval,
    SubprocessExec,
}

#[derive(Debug, Clone)]
enum TaintNode {
    Source {
        kind: SourceKind,
        line: usize,
        label: String,
    },
    Variable {
        name: String,
    },
    Sink {
        kind: SinkKind,
        line: usize,
        callee: String,
    },
}

struct AstSinkRule {
    callees: &'static [&'static str],
    rule_id: &'static str,
    category: FindingCategory,
    severity: FindingSeverity,
    title: &'static str,
    description: &'static str,
    sink_kind: Option<SinkKind>,
    capability: ExercisedCapability,
}

const AST_CALL_RULES: &[AstSinkRule] = &[
    AstSinkRule {
        callees: &["eval", "exec", "compile", "Function", "vm.runInNewContext"],
        rule_id: "AST1",
        category: FindingCategory::AstCodeAnalysis,
        severity: FindingSeverity::High,
        title: "Dynamic Code Evaluation Primitive",
        description: "Script invokes dynamic runtime code evaluation (eval/exec/compile), enabling arbitrary payload execution.",
        sink_kind: Some(SinkKind::DynamicEval),
        capability: ExercisedCapability::Exec,
    },
    AstSinkRule {
        callees: &[
            "__import__",
            "importlib.import_module",
            "getattr",
            "setattr",
            "builtins.__import__",
        ],
        rule_id: "AST2",
        category: FindingCategory::AstCodeAnalysis,
        severity: FindingSeverity::Medium,
        title: "Reflective Symbol or Module Dispatch",
        description: "Script uses reflective dispatch (__import__/getattr) which can obscure dangerous function calls from static allowlists.",
        sink_kind: Some(SinkKind::DynamicEval),
        capability: ExercisedCapability::Exec,
    },
    AstSinkRule {
        callees: &[
            "subprocess.run",
            "subprocess.Popen",
            "subprocess.call",
            "subprocess.check_output",
            "subprocess.check_call",
            "os.system",
            "os.popen",
            "os.execve",
            "os.spawnv",
            "child_process.exec",
            "child_process.execSync",
            "child_process.spawn",
            "Command::new",
        ],
        rule_id: "AST3",
        category: FindingCategory::AstCodeAnalysis,
        severity: FindingSeverity::High,
        title: "Subprocess Command Execution",
        description: "Script spawns an operating system subprocess or shell command.",
        sink_kind: Some(SinkKind::SubprocessExec),
        capability: ExercisedCapability::Exec,
    },
    AstSinkRule {
        callees: &[
            "pickle.loads",
            "pickle.load",
            "marshal.loads",
            "yaml.unsafe_load",
            "shelve.open",
        ],
        rule_id: "AST4",
        category: FindingCategory::AstCodeAnalysis,
        severity: FindingSeverity::Critical,
        title: "Unsafe Object Deserialization",
        description: "Script deserializes untrusted binary/YAML object streams capable of arbitrary code execution.",
        sink_kind: Some(SinkKind::DynamicEval),
        capability: ExercisedCapability::Exec,
    },
    AstSinkRule {
        callees: &[
            "__subclasses__",
            "__bases__",
            "__mro__",
            "__globals__",
            "__builtins__",
            "sys._getframe",
        ],
        rule_id: "AST7",
        category: FindingCategory::SandboxEscape,
        severity: FindingSeverity::Critical,
        title: "Object Hierarchy / Frame Sandbox Escape Introspection",
        description: "Script traverses Python MRO/__subclasses__/__globals__ internals to escape restricted execution namespaces.",
        sink_kind: Some(SinkKind::DynamicEval),
        capability: ExercisedCapability::Exec,
    },
    AstSinkRule {
        callees: &[
            "os.setuid",
            "os.setgid",
            "os.seteuid",
            "os.chroot",
            "ctypes.CDLL",
            "ctypes.windll",
        ],
        rule_id: "SBX1",
        category: FindingCategory::SandboxEscape,
        severity: FindingSeverity::Critical,
        title: "Native FFI / Kernel Privilege Manipulation",
        description: "Script invokes raw C FFI or OS privilege/chroot syscalls that can bypass runtime sandboxing.",
        sink_kind: None,
        capability: ExercisedCapability::Exec,
    },
];

struct TaintFlowRule {
    source: SourceKind,
    sink: SinkKind,
    rule_id: &'static str,
    severity: FindingSeverity,
    confidence: f64,
    title: &'static str,
    description: &'static str,
}

/// Declarative `(SourceKind, SinkKind)` taint matrix (`TT1`..`TT5`).
const TAINT_FLOW_MATRIX: &[TaintFlowRule] = &[
    TaintFlowRule {
        source: SourceKind::EnvSecret,
        sink: SinkKind::NetworkExfil,
        rule_id: "TT1",
        severity: FindingSeverity::Critical,
        confidence: 0.97,
        title: "Source-to-Sink Environment Secret Exfiltration Flow",
        description: "Intraprocedural AST taint analysis proved a reachable dataflow path from an environment secret source to an outbound network sink.",
    },
    TaintFlowRule {
        source: SourceKind::CredentialFile,
        sink: SinkKind::NetworkExfil,
        rule_id: "TT2",
        severity: FindingSeverity::Critical,
        confidence: 0.97,
        title: "Source-to-Sink Credential File Exfiltration Flow",
        description: "Intraprocedural AST taint analysis proved a reachable dataflow path from a host credential/key file into an outbound network sink.",
    },
    TaintFlowRule {
        source: SourceKind::NetworkInput,
        sink: SinkKind::DynamicEval,
        rule_id: "TT3",
        severity: FindingSeverity::Critical,
        confidence: 0.96,
        title: "Remote Network-to-Eval Execution Flow",
        description: "Intraprocedural AST taint analysis proved that data fetched from an external network source flows into a dynamic code evaluation sink.",
    },
    TaintFlowRule {
        source: SourceKind::NetworkInput,
        sink: SinkKind::SubprocessExec,
        rule_id: "TT3",
        severity: FindingSeverity::Critical,
        confidence: 0.96,
        title: "Remote Network-to-Subprocess Execution Flow",
        description: "Intraprocedural AST taint analysis proved that data fetched from an external network source flows into a subprocess execution sink.",
    },
    TaintFlowRule {
        source: SourceKind::EnvSecret,
        sink: SinkKind::DynamicEval,
        rule_id: "TT4",
        severity: FindingSeverity::High,
        confidence: 0.91,
        title: "Tainted Secret Flow Into Dynamic Eval Sink",
        description: "Intraprocedural AST taint analysis proved that environment secret data flows into a dynamic evaluation sink.",
    },
    TaintFlowRule {
        source: SourceKind::EnvSecret,
        sink: SinkKind::SubprocessExec,
        rule_id: "TT4",
        severity: FindingSeverity::High,
        confidence: 0.90,
        title: "Tainted Secret Flow Into Subprocess Sink",
        description: "Intraprocedural AST taint analysis proved that environment secret data is interpolated into a subprocess command sink.",
    },
    TaintFlowRule {
        source: SourceKind::CredentialFile,
        sink: SinkKind::DynamicEval,
        rule_id: "TT4",
        severity: FindingSeverity::High,
        confidence: 0.91,
        title: "Tainted Credential File Flow Into Eval Sink",
        description: "Intraprocedural AST taint analysis proved that sensitive file data flows into a dynamic evaluation sink.",
    },
    TaintFlowRule {
        source: SourceKind::CredentialFile,
        sink: SinkKind::SubprocessExec,
        rule_id: "TT4",
        severity: FindingSeverity::High,
        confidence: 0.90,
        title: "Tainted Credential File Flow Into Subprocess Sink",
        description: "Intraprocedural AST taint analysis proved that sensitive file data flows into a subprocess sink.",
    },
    TaintFlowRule {
        source: SourceKind::CommandOutput,
        sink: SinkKind::NetworkExfil,
        rule_id: "TT5",
        severity: FindingSeverity::High,
        confidence: 0.93,
        title: "Subprocess Reconnaissance Output-to-Network Exfiltration Flow",
        description: "Intraprocedural AST taint analysis proved that local command execution output flows into an outbound network transmission sink.",
    },
];

const NETWORK_EXFIL_CALLEES: &[&str] = &[
    "requests.post",
    "requests.put",
    "requests.patch",
    "requests.request",
    "urllib.request.urlopen",
    "urllib.request.Request",
    "httpx.post",
    "httpx.AsyncClient.post",
    "aiohttp.ClientSession.post",
    "socket.connect",
    "socket.sendall",
    "fetch",
    "axios.post",
    "axios.put",
    "http.request",
    "https.request",
    "navigator.sendBeacon",
];

const NETWORK_INPUT_CALLEES: &[&str] = &[
    "requests.get",
    "urllib.request.urlopen",
    "urllib.request.urlretrieve",
    "httpx.get",
    "fetch",
    "axios.get",
];

const COMMAND_OUTPUT_CALLEES: &[&str] = &[
    "subprocess.check_output",
    "subprocess.run",
    "subprocess.Popen",
    "os.popen",
    "child_process.execSync",
];

const ENV_SOURCE_TOKENS: &[&str] = &[
    "os.environ",
    "os.getenv",
    "process.env",
    "std::env::var",
    "env::var",
    "dotenv.dotenv_values",
];

const SENSITIVE_PATH_MARKERS: &[&str] = &[
    ".ssh/",
    "id_rsa",
    "id_ed25519",
    ".aws/credentials",
    ".gnupg",
    ".kube/config",
    "/etc/shadow",
    "/etc/passwd",
    ".npmrc",
    ".pypirc",
    ".netrc",
];

const SQL_STATEMENT_VERBS: &[&str] = &["SELECT ", "INSERT ", "UPDATE ", "DELETE ", "DROP "];

struct ShellStageRule {
    rule_id: &'static str,
    category: FindingCategory,
    severity: FindingSeverity,
    confidence: f64,
    title: &'static str,
    description: &'static str,
    remediation: &'static str,
    cmd_matches: &'static [&'static str],
    arg_markers: &'static [&'static str],
}

/// Declarative shell pipeline stage rule table (`PE1`, `RA2`, `SC1`).
const SHELL_STAGE_RULES: &[ShellStageRule] = &[
    ShellStageRule {
        rule_id: "PE1",
        category: FindingCategory::PrivilegeEscalation,
        severity: FindingSeverity::High,
        confidence: 0.93,
        title: "Privilege Escalation Command",
        description: "Command invokes root privilege escalation (sudo/su/doas/pkexec or setuid chmod).",
        remediation: "Run skill commands with standard unprivileged user permissions.",
        cmd_matches: &["sudo", "su", "doas", "pkexec"],
        arg_markers: &["+s", "4755", "u+s"],
    },
    ShellStageRule {
        rule_id: "RA2",
        category: FindingCategory::RogueAgent,
        severity: FindingSeverity::Critical,
        confidence: 0.94,
        title: "Host Persistence Mechanism Modification",
        description: "Command modifies cron schedules, shell initialization profiles, systemd units, or SSH authorized_keys.",
        remediation: "Never install persistent background hooks or modify user shell initialization files.",
        cmd_matches: &["crontab", "systemctl", "launchctl"],
        arg_markers: &[
            ".bashrc",
            ".zshrc",
            ".profile",
            "authorized_keys",
            "/etc/systemd",
            "/etc/cron",
            "LaunchAgents",
        ],
    },
    ShellStageRule {
        rule_id: "SC1",
        category: FindingCategory::SupplyChain,
        severity: FindingSeverity::High,
        confidence: 0.92,
        title: "Untrusted Package Index or Registry Redirection",
        description: "Package installation command overrides the canonical package index URL or disables TLS host trust.",
        remediation: "Install dependencies exclusively from canonical, TLS-verified package registries.",
        cmd_matches: &[],
        arg_markers: &[
            "--index-url",
            "--extra-index-url",
            "--trusted-host",
            "--find-links",
            "--registry",
        ],
    },
];

const SHELL_FETCHER_CMDS: &[&str] = &["curl", "wget", "fetch"];
const SHELL_INTERPRETER_CMDS: &[&str] = &[
    "sh", "bash", "zsh", "dash", "python", "python3", "node", "eval", "source",
];
const SHELL_NET_SEND_FLAGS: &[&str] = &[
    "-d",
    "--data",
    "--data-binary",
    "--data-raw",
    "-F",
    "--upload-file",
    "POST",
];
const SHELL_RAW_SOCKET_CMDS: &[&str] = &["nc", "ncat", "netcat", "socat", "scp"];

/// Resolves a Tree-sitter `Language` grammar for supported script extensions or code fence tags.
pub fn language_for_script_tag(ext_or_tag: &str) -> Option<Language> {
    match ext_or_tag.trim().to_ascii_lowercase().as_str() {
        "py" | "python" | "python3" => Some(tree_sitter_python::LANGUAGE.into()),
        "js" | "jsx" | "mjs" | "cjs" | "javascript" | "node" => {
            Some(tree_sitter_javascript::LANGUAGE.into())
        }
        "ts" | "mts" | "cts" | "typescript" => {
            Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        }
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "rs" | "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        _ => None,
    }
}

/// Analyzes a script file or code block using Tree-sitter CST traversal and `petgraph` taint reachability.
pub fn analyze_script_ast_and_taint(
    rel_path: &str,
    ext_or_tag: &str,
    code: &str,
    base_line_offset: usize,
) -> (Vec<SkillFinding>, HashSet<ExercisedCapability>) {
    let mut findings = Vec::new();
    let mut capabilities = HashSet::new();

    if matches!(
        ext_or_tag.trim().to_ascii_lowercase().as_str(),
        "sh" | "bash" | "zsh" | "shell" | "ps1"
    ) {
        return analyze_shell_pipeline(rel_path, code, base_line_offset);
    }

    let Some(language) = language_for_script_tag(ext_or_tag) else {
        return (findings, capabilities);
    };

    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return (findings, capabilities);
    }
    let Some(tree) = parser.parse(code, None) else {
        return (findings, capabilities);
    };

    let mut state = AstTaintBuilder::new(rel_path, code, base_line_offset);
    state.walk_node(tree.root_node());
    state.evaluate_taint_reachability(&mut findings);

    findings.extend(state.structural_findings);
    capabilities.extend(state.capabilities);
    (findings, capabilities)
}

struct AstTaintBuilder<'a> {
    rel_path: &'a str,
    code: &'a str,
    base_line_offset: usize,
    aliases: HashMap<String, String>,
    graph: DiGraph<TaintNode, ()>,
    var_nodes: HashMap<String, NodeIndex>,
    sources: Vec<NodeIndex>,
    sinks: Vec<NodeIndex>,
    structural_findings: Vec<SkillFinding>,
    capabilities: HashSet<ExercisedCapability>,
}

impl<'a> AstTaintBuilder<'a> {
    fn new(rel_path: &'a str, code: &'a str, base_line_offset: usize) -> Self {
        Self {
            rel_path,
            code,
            base_line_offset,
            aliases: HashMap::new(),
            graph: DiGraph::new(),
            var_nodes: HashMap::new(),
            sources: Vec::new(),
            sinks: Vec::new(),
            structural_findings: Vec::new(),
            capabilities: HashSet::new(),
        }
    }

    fn var_node(&mut self, name: &str) -> NodeIndex {
        if let Some(&idx) = self.var_nodes.get(name) {
            return idx;
        }
        let idx = self.graph.add_node(TaintNode::Variable {
            name: name.to_string(),
        });
        self.var_nodes.insert(name.to_string(), idx);
        idx
    }

    fn add_source(&mut self, kind: SourceKind, line: usize, label: String) -> NodeIndex {
        let idx = self.graph.add_node(TaintNode::Source { kind, line, label });
        self.sources.push(idx);
        idx
    }

    fn add_sink(&mut self, kind: SinkKind, line: usize, callee: String) -> NodeIndex {
        let idx = self.graph.add_node(TaintNode::Sink { kind, line, callee });
        self.sinks.push(idx);
        idx
    }

    fn walk_node(&mut self, node: Node<'a>) {
        match node.kind() {
            "import_statement" | "import_from_statement" => {
                self.extract_python_import_alias(node);
            }
            "assignment" | "augmented_assignment" | "variable_declarator" | "let_declaration" => {
                self.process_assignment(node);
            }
            "call" | "call_expression" | "new_expression" => {
                self.process_call(node);
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_node(child);
        }
    }

    fn extract_python_import_alias(&mut self, node: Node<'a>) {
        let Ok(text) = node.utf8_text(self.code.as_bytes()) else {
            return;
        };
        let tokens: Vec<&str> = text.split_whitespace().collect();
        for win in tokens.windows(4) {
            if win[0] == "import" && win[2] == "as" {
                self.aliases.insert(
                    win[3].trim_end_matches(',').to_string(),
                    win[1].trim_end_matches(',').to_string(),
                );
            }
        }
        if tokens.len() >= 6 && tokens[0] == "from" && tokens[2] == "import" && tokens[4] == "as" {
            let full = format!("{}.{}", tokens[1], tokens[3]);
            self.aliases
                .insert(tokens[5].trim_end_matches(',').to_string(), full);
        }
    }

    fn normalize_callee(&self, raw: &str) -> String {
        let trimmed = raw.trim();
        if let Some((prefix, rest)) = trimmed.split_once('.') {
            if let Some(mapped) = self.aliases.get(prefix) {
                return format!("{}.{}", mapped, rest);
            }
        } else if let Some(mapped) = self.aliases.get(trimmed) {
            return mapped.clone();
        }
        trimmed.to_string()
    }

    fn process_assignment(&mut self, node: Node<'a>) {
        let line = self.base_line_offset + node.start_position().row + 1;
        let lhs_node = node
            .child_by_field_name("left")
            .or_else(|| node.child_by_field_name("name"))
            .or_else(|| node.child_by_field_name("pattern"))
            .or_else(|| node.named_child(0));
        let rhs_node = node
            .child_by_field_name("right")
            .or_else(|| node.child_by_field_name("value"))
            .or_else(|| node.named_child(1));

        let (Some(lhs), Some(rhs)) = (lhs_node, rhs_node) else {
            return;
        };

        let mut lhs_vars = Vec::new();
        Self::collect_identifiers(lhs, self.code, &mut lhs_vars);
        if lhs_vars.is_empty() {
            return;
        }

        let rhs_text = rhs.utf8_text(self.code.as_bytes()).unwrap_or("");
        let norm_rhs = self.normalize_callee(rhs_text);

        let mut source_indices = Vec::new();
        if ENV_SOURCE_TOKENS
            .iter()
            .any(|s| rhs_text.contains(s) || norm_rhs.contains(s))
        {
            self.capabilities.insert(ExercisedCapability::EnvRead);
            source_indices.push(self.add_source(
                SourceKind::EnvSecret,
                line,
                "environment_secret".to_string(),
            ));
        }
        if SENSITIVE_PATH_MARKERS.iter().any(|m| rhs_text.contains(m)) {
            self.capabilities.insert(ExercisedCapability::Read);
            source_indices.push(self.add_source(
                SourceKind::CredentialFile,
                line,
                "credential_file".to_string(),
            ));
        }
        if NETWORK_INPUT_CALLEES
            .iter()
            .any(|c| rhs_text.contains(c) || norm_rhs.contains(c))
        {
            self.capabilities.insert(ExercisedCapability::Network);
            source_indices.push(self.add_source(
                SourceKind::NetworkInput,
                line,
                "network_response".to_string(),
            ));
        }
        if COMMAND_OUTPUT_CALLEES
            .iter()
            .any(|c| rhs_text.contains(c) || norm_rhs.contains(c))
        {
            self.capabilities.insert(ExercisedCapability::Exec);
            source_indices.push(self.add_source(
                SourceKind::CommandOutput,
                line,
                "command_output".to_string(),
            ));
        }

        let mut rhs_vars = Vec::new();
        Self::collect_identifiers(rhs, self.code, &mut rhs_vars);

        for lhs_name in &lhs_vars {
            let lhs_idx = self.var_node(lhs_name);
            for &src_idx in &source_indices {
                self.graph.update_edge(src_idx, lhs_idx, ());
            }
            for rhs_name in &rhs_vars {
                if rhs_name != lhs_name {
                    let rhs_idx = self.var_node(rhs_name);
                    self.graph.update_edge(rhs_idx, lhs_idx, ());
                }
            }
        }
    }

    fn process_call(&mut self, node: Node<'a>) {
        let line = self.base_line_offset + node.start_position().row + 1;
        let Some(func_node) = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0))
        else {
            return;
        };
        let raw_callee = func_node.utf8_text(self.code.as_bytes()).unwrap_or("");
        let callee = self.normalize_callee(raw_callee);
        let full_call_text = node.utf8_text(self.code.as_bytes()).unwrap_or("");

        // 1. Check sensitive credential file access in open/read calls (E3)
        if SENSITIVE_PATH_MARKERS
            .iter()
            .any(|m| full_call_text.contains(m))
        {
            self.capabilities.insert(ExercisedCapability::Read);
            self.structural_findings.push(SkillFinding::new(
                "E3",
                FindingCategory::CredentialExposure,
                FindingSeverity::High,
                0.92,
                self.rel_path,
                line,
                "Sensitive Credential Path Access",
                "Script accesses a host credential store or private key path (.ssh, .aws, .gnupg, /etc/shadow).",
                Self::compact_snippet(full_call_text),
                "Do not read host private keys or credential files directly from skill scripts.",
            ));
        }

        // 2. Check structural AST rules table (`AST1`..`AST4`, `AST7`, `SBX1`)
        let mut matched_sink_kind = None;
        for rule in AST_CALL_RULES {
            if rule.callees.iter().any(|&c| {
                callee == c
                    || callee.ends_with(&format!(".{}", c))
                    || (c.contains('.') && callee.ends_with(c))
            }) {
                self.capabilities.insert(rule.capability);
                matched_sink_kind = matched_sink_kind.or(rule.sink_kind);
                self.structural_findings.push(SkillFinding::new(
                    rule.rule_id,
                    rule.category,
                    rule.severity,
                    0.91,
                    self.rel_path,
                    line,
                    rule.title,
                    rule.description,
                    Self::compact_snippet(full_call_text),
                    "Eliminate dynamic code execution or arbitrary subprocess dispatch; use declarative tool APIs.",
                ));
            }
        }

        let args_node = node
            .child_by_field_name("arguments")
            .or_else(|| node.named_child(1));

        // 3. AST5: Check Python `shell=True` keyword argument on subprocess calls (`static_python_shell_truthiness.py`)
        if matched_sink_kind == Some(SinkKind::SubprocessExec) {
            if let Some(args) = args_node {
                if Self::has_shell_true_kwarg(args, self.code) {
                    self.structural_findings.push(SkillFinding::new(
                        "AST5",
                        FindingCategory::AstCodeAnalysis,
                        FindingSeverity::Critical,
                        0.95,
                        self.rel_path,
                        line,
                        "Subprocess Invoked With `shell=True`",
                        "Subprocess call enables `shell=True`, allowing shell metacharacter command injection.",
                        Self::compact_snippet(full_call_text),
                        "Pass command arguments as an explicit list with `shell=False`.",
                    ));
                }
            }
        }

        // 4. AST6: Check unbound SQL string interpolation in `.execute(...)` (`sql_parameter_binding.py`)
        if callee.ends_with(".execute") || callee.ends_with(".executemany") {
            if let Some(args) = args_node {
                if Self::has_unbound_sql_interpolation(args, self.code) {
                    self.structural_findings.push(SkillFinding::new(
                        "AST6",
                        FindingCategory::AstCodeAnalysis,
                        FindingSeverity::High,
                        0.90,
                        self.rel_path,
                        line,
                        "Unbound SQL Query String Interpolation",
                        "Database `.execute()` call constructs SQL via f-string, `%`, or `+` concatenation instead of parameterized bindings.",
                        Self::compact_snippet(full_call_text),
                        "Use parameterized query placeholders (`?` or `%s` with a bound parameter tuple).",
                    ));
                }
            }
        }

        if NETWORK_EXFIL_CALLEES.iter().any(|&c| {
            callee == c
                || callee.ends_with(&format!(".{}", c))
                || (c.contains('.') && callee.ends_with(c))
        }) {
            self.capabilities.insert(ExercisedCapability::Network);
            matched_sink_kind = Some(SinkKind::NetworkExfil);
        }

        if let Some(sink_kind) = matched_sink_kind {
            let sink_idx = self.add_sink(sink_kind, line, callee);
            if let Some(args) = args_node {
                let args_text = args.utf8_text(self.code.as_bytes()).unwrap_or("");
                if ENV_SOURCE_TOKENS.iter().any(|s| args_text.contains(s)) {
                    let src_idx = self.add_source(
                        SourceKind::EnvSecret,
                        line,
                        "environment_secret".to_string(),
                    );
                    self.graph.update_edge(src_idx, sink_idx, ());
                }
                if SENSITIVE_PATH_MARKERS.iter().any(|m| args_text.contains(m)) {
                    let src_idx = self.add_source(
                        SourceKind::CredentialFile,
                        line,
                        "credential_file".to_string(),
                    );
                    self.graph.update_edge(src_idx, sink_idx, ());
                }
                let mut arg_vars = Vec::new();
                Self::collect_identifiers(args, self.code, &mut arg_vars);
                for var_name in arg_vars {
                    let v_idx = self.var_node(&var_name);
                    self.graph.update_edge(v_idx, sink_idx, ());
                }
            }
        }
    }

    fn has_shell_true_kwarg(args_node: Node<'a>, code: &'a str) -> bool {
        let mut cursor = args_node.walk();
        for child in args_node.children(&mut cursor) {
            if child.kind() == "keyword_argument" {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(code.as_bytes()).ok())
                    .unwrap_or("");
                let val = child
                    .child_by_field_name("value")
                    .and_then(|v| v.utf8_text(code.as_bytes()).ok())
                    .unwrap_or("");
                if name.trim() == "shell" && matches!(val.trim(), "True" | "1" | "true") {
                    return true;
                }
            }
        }
        false
    }

    fn has_unbound_sql_interpolation(args_node: Node<'a>, code: &'a str) -> bool {
        let Some(first_arg) = args_node.named_child(0) else {
            return false;
        };
        let raw = first_arg.utf8_text(code.as_bytes()).unwrap_or("");
        let upper = raw.to_ascii_uppercase();
        let has_sql_verb = SQL_STATEMENT_VERBS.iter().any(|v| upper.contains(v));
        let is_interpolated = matches!(
            first_arg.kind(),
            "binary_operator" | "template_string" | "call"
        ) || ((raw.starts_with("f\"") || raw.starts_with("f'"))
            && raw.contains('{'));
        has_sql_verb && is_interpolated
    }

    fn collect_identifiers(node: Node<'a>, code: &'a str, out: &mut Vec<String>) {
        if node.kind() == "identifier" {
            if let Ok(name) = node.utf8_text(code.as_bytes()) {
                let trimmed = name.trim();
                if !trimmed.is_empty() && !out.iter().any(|x| x == trimmed) {
                    out.push(trimmed.to_string());
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != "function" {
                Self::collect_identifiers(child, code, out);
            }
        }
    }

    fn evaluate_taint_reachability(&self, findings: &mut Vec<SkillFinding>) {
        for &src_idx in &self.sources {
            let TaintNode::Source {
                kind: src_kind,
                line: src_line,
                ..
            } = self.graph[src_idx]
            else {
                continue;
            };

            for &sink_idx in &self.sinks {
                let TaintNode::Sink {
                    kind: sink_kind,
                    line: sink_line,
                    ..
                } = self.graph[sink_idx]
                else {
                    continue;
                };

                let Some(rule) = TAINT_FLOW_MATRIX
                    .iter()
                    .find(|r| r.source == src_kind && r.sink == sink_kind)
                else {
                    continue;
                };

                if let Some((_, path)) = astar(
                    &self.graph,
                    src_idx,
                    |n| n == sink_idx,
                    |_| 1usize,
                    |_| 0usize,
                ) {
                    let chain: Vec<String> = path
                        .iter()
                        .map(|&idx| match &self.graph[idx] {
                            TaintNode::Source { label, line, .. } => {
                                format!("{}(L{})", label, line)
                            }
                            TaintNode::Variable { name } => format!("${}", name),
                            TaintNode::Sink { callee, line, .. } => {
                                format!("{}(L{})", callee, line)
                            }
                        })
                        .collect();

                    findings.push(SkillFinding::new(
                        rule.rule_id,
                        FindingCategory::TaintTracking,
                        rule.severity,
                        rule.confidence,
                        self.rel_path,
                        sink_line.max(src_line),
                        rule.title,
                        rule.description,
                        chain.join(" -> "),
                        "Sever the dataflow path between untrusted/sensitive sources and execution or network sinks.",
                    ));
                }
            }
        }
    }

    fn compact_snippet(s: &str) -> String {
        let single_line: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if single_line.len() > SKILLSPECTOR_COMPACT_SNIPPET_LEN {
            format!("{}...", &single_line[..SKILLSPECTOR_COMPACT_SNIPPET_LEN])
        } else {
            single_line
        }
    }
}

/// Analyzes shell command pipelines (`|`, `;`, `&&`) for stage-to-stage flow vulnerabilities (`SC1`, `SC2`, `E1`, `PE1`, `RA2`).
pub fn analyze_shell_pipeline(
    rel_path: &str,
    code: &str,
    base_line_offset: usize,
) -> (Vec<SkillFinding>, HashSet<ExercisedCapability>) {
    let mut findings = Vec::new();
    let mut capabilities = HashSet::new();

    for (idx, raw_line) in code.lines().enumerate() {
        let line_num = base_line_offset + idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let stages: Vec<&str> = line.split('|').map(str::trim).collect();
        let mut stage_has_fetcher = false;
        let mut stage_has_secret_source = false;

        for stage in &stages {
            let tokens: Vec<&str> = stage.split_whitespace().collect();
            let Some(&cmd) = tokens.first() else {
                continue;
            };
            capabilities.insert(ExercisedCapability::Exec);

            let is_fetcher = SHELL_FETCHER_CMDS.contains(&cmd);
            let is_interpreter = SHELL_INTERPRETER_CMDS.contains(&cmd)
                || tokens.iter().any(|t| SHELL_INTERPRETER_CMDS.contains(t));
            let is_net_send = (SHELL_FETCHER_CMDS.contains(&cmd)
                && tokens.iter().any(|t| SHELL_NET_SEND_FLAGS.contains(t)))
                || SHELL_RAW_SOCKET_CMDS.contains(&cmd);
            let upper_stage = stage.to_ascii_uppercase();
            let has_secret_ref = SENSITIVE_PATH_MARKERS.iter().any(|m| stage.contains(m))
                || matches!(cmd, "env" | "printenv")
                || (stage.contains('$')
                    && SECRET_PATTERNS.iter().any(|pat| upper_stage.contains(pat)));

            if is_fetcher || is_net_send {
                capabilities.insert(ExercisedCapability::Network);
            }
            if stage_has_fetcher && is_interpreter {
                findings.push(SkillFinding::new(
                    "SC2",
                    FindingCategory::SupplyChain,
                    FindingSeverity::Critical,
                    0.98,
                    rel_path,
                    line_num,
                    "Remote Script Pipe-to-Shell Execution",
                    "Pipeline fetches remote content over the network and pipes it directly into a shell or language interpreter.",
                    line.to_string(),
                    "Download scripts to a temporary file, verify cryptographic checksums, and inspect before execution.",
                ));
            }
            if (stage_has_secret_source || has_secret_ref) && is_net_send {
                findings.push(SkillFinding::new(
                    "E1",
                    FindingCategory::DataExfiltration,
                    FindingSeverity::Critical,
                    0.96,
                    rel_path,
                    line_num,
                    "Shell Pipeline Credential Exfiltration",
                    "Shell pipeline reads host credentials or environment variables and transmits them over the network.",
                    line.to_string(),
                    "Remove outbound network transmission of environment variables or credential files.",
                ));
            }

            for rule in SHELL_STAGE_RULES {
                let cmd_hit = rule.cmd_matches.contains(&cmd);
                let arg_hit = rule.arg_markers.iter().any(|m| stage.contains(m));
                if cmd_hit || arg_hit {
                    findings.push(SkillFinding::new(
                        rule.rule_id,
                        rule.category,
                        rule.severity,
                        rule.confidence,
                        rel_path,
                        line_num,
                        rule.title,
                        rule.description,
                        line.to_string(),
                        rule.remediation,
                    ));
                }
            }

            stage_has_fetcher |= is_fetcher;
            stage_has_secret_source |= has_secret_ref;
        }
    }

    (findings, capabilities)
}
