use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEdge {
    pub caller: String,
    pub callee: String,
    pub call_type: CallType,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CallType {
    DirectInvoke,
    ClientNew,
    ExternalCall,
    InternalCall,
}

impl std::fmt::Display for CallType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallType::DirectInvoke => write!(f, "invoke"),
            CallType::ClientNew => write!(f, "client"),
            CallType::ExternalCall => write!(f, "external"),
            CallType::InternalCall => write!(f, "internal"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallNode {
    pub name: String,
    pub functions: Vec<String>,
    pub is_external: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallGraph {
    pub root: String,
    pub nodes: Vec<CallNode>,
    pub edges: Vec<CallEdge>,
    pub dependencies: Vec<String>,
    pub patterns: Vec<CallPattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallPattern {
    pub name: String,
    pub description: String,
    pub severity: String,
}

pub fn extract_call_graph(path: &Path) -> Result<CallGraph> {
    let content = fs::read_to_string(path)?;
    let root = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("contract")
        .to_string();

    let edges = extract_edges(&content, &root);
    let nodes = build_nodes(&edges, &root);
    let dependencies = extract_dependencies(&content);
    let patterns = detect_patterns(&edges, &content);

    Ok(CallGraph {
        root,
        nodes,
        edges,
        dependencies,
        patterns,
    })
}

fn extract_edges(content: &str, root: &str) -> Vec<CallEdge> {
    let mut edges = Vec::new();

    // Pattern 1: invoke_contract! macro
    let invoke_pattern = "invoke_contract!";
    let mut search = content;
    while let Some(pos) = search.find(invoke_pattern) {
        let rest = &search[pos + invoke_pattern.len()..];
        let callee = extract_contract_arg(rest).unwrap_or_else(|| "unknown".to_string());
        let line = count_lines(&content[..content.len() - search.len() + pos]);
        edges.push(CallEdge {
            caller: root.to_string(),
            callee,
            call_type: CallType::DirectInvoke,
            location: Some(format!("line {}", line)),
        });
        search = &search[1..];
    }

    // Pattern 2: Client::new(env, contract_id)
    let client_pattern = "Client::new";
    let mut search = content;
    while let Some(pos) = search.find(client_pattern) {
        let prefix = &content[..content.len() - search.len() + pos];
        let callee = extract_client_name(prefix).unwrap_or_else(|| "ExternalContract".to_string());
        let line = count_lines(prefix);
        edges.push(CallEdge {
            caller: root.to_string(),
            callee,
            call_type: CallType::ClientNew,
            location: Some(format!("line {}", line)),
        });
        search = &search[1..];
    }

    // Pattern 3: contract::Client or ContractName::Client
    let client_suffix = "::Client";
    let mut search = content;
    while let Some(pos) = search.find(client_suffix) {
        let prefix_area = &content[..content.len() - search.len() + pos];
        if let Some(callee) = extract_module_name(prefix_area) {
            let already = edges.iter().any(|e| e.callee == callee);
            if !already {
                let line = count_lines(prefix_area);
                edges.push(CallEdge {
                    caller: root.to_string(),
                    callee,
                    call_type: CallType::ExternalCall,
                    location: Some(format!("line {}", line)),
                });
            }
        }
        search = &search[1..];
    }

    // Pattern 4: internal fn calls (fn name in same file)
    let fns = extract_function_names(content);
    for fn_name in &fns {
        let call_pattern = format!("{}(", fn_name);
        let definitions = content.matches(&format!("fn {}(", fn_name)).count();
        let calls = content.matches(&call_pattern).count();
        if calls > definitions && fn_name != root {
            edges.push(CallEdge {
                caller: root.to_string(),
                callee: fn_name.clone(),
                call_type: CallType::InternalCall,
                location: None,
            });
        }
    }

    edges
}

fn extract_contract_arg(text: &str) -> Option<String> {
    let start = text.find('(')?;
    let rest = &text[start + 1..];
    let end = rest.find(',')?;
    let raw = rest[..end].trim().trim_matches('&').trim();
    if raw.is_empty() || raw == "env" {
        None
    } else {
        Some(raw.to_string())
    }
}

fn extract_client_name(prefix: &str) -> Option<String> {
    let parts: Vec<&str> = prefix
        .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
        .collect();
    parts
        .into_iter()
        .find(|s| !s.is_empty() && s.chars().next().is_some_and(|c| c.is_uppercase()))
        .map(|s| s.to_string())
}

fn extract_module_name(prefix: &str) -> Option<String> {
    let last_alpha: String = prefix
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if last_alpha.is_empty() || last_alpha.chars().next().is_none_or(|c| c.is_lowercase()) {
        None
    } else {
        Some(last_alpha)
    }
}

fn extract_function_names(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") {
            let after_fn = trimmed.trim_start_matches("pub ").trim_start_matches("fn ");
            if let Some(paren) = after_fn.find('(') {
                let name = after_fn[..paren].trim();
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

fn build_nodes(edges: &[CallEdge], root: &str) -> Vec<CallNode> {
    let mut node_map: HashMap<String, (Vec<String>, bool)> = HashMap::new();

    node_map
        .entry(root.to_string())
        .or_insert_with(|| (Vec::new(), false));

    for edge in edges {
        let is_external = edge.call_type != CallType::InternalCall;
        let entry = node_map
            .entry(edge.callee.clone())
            .or_insert_with(|| (Vec::new(), is_external));
        entry.1 = entry.1 || is_external;
    }

    node_map
        .into_iter()
        .map(|(name, (functions, is_external))| CallNode {
            name,
            functions,
            is_external,
        })
        .collect()
}

fn extract_dependencies(content: &str) -> Vec<String> {
    let mut deps = HashSet::new();
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("use ") {
            let without_use = t.trim_start_matches("use ").trim_end_matches(';');
            if let Some(top) = without_use.split("::").next() {
                if top != "crate" && top != "super" && top != "self" && top != "std" {
                    deps.insert(top.to_string());
                }
            }
        }
    }
    deps.into_iter().collect()
}

fn detect_patterns(edges: &[CallEdge], content: &str) -> Vec<CallPattern> {
    let mut patterns = Vec::new();

    // Check for re-entrancy risk: calling external contract then updating state
    let has_external_calls = edges.iter().any(|e| e.call_type != CallType::InternalCall);
    let has_storage_after = content.contains("storage.set")
        || content.contains("env.storage().set")
        || content.contains(".set(");
    if has_external_calls && has_storage_after {
        patterns.push(CallPattern {
            name: "potential-reentrancy".to_string(),
            description: "External calls detected before storage updates — consider using checks-effects-interactions pattern.".to_string(),
            severity: "medium".to_string(),
        });
    }

    // Check for deep call chains
    if edges.len() > 5 {
        patterns.push(CallPattern {
            name: "deep-call-chain".to_string(),
            description: format!(
                "Contract has {} outgoing calls. Deep call chains increase gas cost and attack surface.",
                edges.len()
            ),
            severity: "low".to_string(),
        });
    }

    // Check for missing auth on external calls
    let has_require_auth =
        content.contains("require_auth") || content.contains("require_auth_for_args");
    if has_external_calls && !has_require_auth {
        patterns.push(CallPattern {
            name: "missing-auth-check".to_string(),
            description: "External calls found but no require_auth() detected. Ensure callers are authorized.".to_string(),
            severity: "high".to_string(),
        });
    }

    patterns
}

fn count_lines(text: &str) -> usize {
    text.lines().count() + 1
}

pub fn render_ascii(graph: &CallGraph) -> String {
    let mut out = String::new();
    out.push_str(&format!("Call Graph: {}\n", graph.root));
    out.push_str(&"─".repeat(50));
    out.push('\n');

    let external: Vec<_> = graph
        .edges
        .iter()
        .filter(|e| e.call_type != CallType::InternalCall)
        .collect();
    let internal: Vec<_> = graph
        .edges
        .iter()
        .filter(|e| e.call_type == CallType::InternalCall)
        .collect();

    if !external.is_empty() {
        out.push_str("\nExternal Calls:\n");
        for edge in &external {
            let loc = edge.location.as_deref().unwrap_or("").to_string();
            out.push_str(&format!(
                "  [{}] ──({})──▶ {}  {}\n",
                graph.root, edge.call_type, edge.callee, loc,
            ));
        }
    }

    if !internal.is_empty() {
        out.push_str("\nInternal Functions:\n");
        for edge in &internal {
            out.push_str(&format!("  [{}] calls {}()\n", graph.root, edge.callee));
        }
    }

    if !graph.dependencies.is_empty() {
        out.push_str("\nImport Dependencies:\n");
        for dep in &graph.dependencies {
            out.push_str(&format!("  use {}\n", dep));
        }
    }

    if !graph.patterns.is_empty() {
        out.push_str("\nPatterns Detected:\n");
        for pat in &graph.patterns {
            out.push_str(&format!(
                "  [{}] {}: {}\n",
                pat.severity.to_uppercase(),
                pat.name,
                pat.description,
            ));
        }
    }

    out.push_str(&"─".repeat(50));
    out.push('\n');
    out
}

pub fn render_dot(graph: &CallGraph) -> String {
    let mut out = String::new();
    out.push_str("digraph call_graph {\n");
    out.push_str("  rankdir=LR;\n");
    out.push_str("  node [shape=box];\n");
    out.push_str(&format!(
        "  \"{}\" [style=filled, fillcolor=lightblue];\n",
        graph.root
    ));

    let mut seen = HashSet::new();
    for edge in &graph.edges {
        if !seen.contains(&edge.callee) {
            seen.insert(edge.callee.clone());
            let color = if edge.call_type == CallType::InternalCall {
                "lightyellow"
            } else {
                "lightcoral"
            };
            out.push_str(&format!(
                "  \"{}\" [style=filled, fillcolor={}];\n",
                edge.callee, color
            ));
        }
        let style = match edge.call_type {
            CallType::InternalCall => "dashed",
            _ => "solid",
        };
        out.push_str(&format!(
            "  \"{}\" -> \"{}\" [label=\"{}\", style={}];\n",
            edge.caller, edge.callee, edge.call_type, style,
        ));
    }

    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_functions_finds_pub_fn() {
        let src = "pub fn transfer(env: Env) {}\nfn helper() {}";
        let fns = extract_function_names(src);
        assert!(fns.contains(&"transfer".to_string()));
        assert!(fns.contains(&"helper".to_string()));
    }

    #[test]
    fn deps_excludes_std_and_crate() {
        let src = "use crate::utils;\nuse std::vec;\nuse soroban_sdk::Env;";
        let deps = extract_dependencies(src);
        assert!(!deps.contains(&"crate".to_string()));
        assert!(!deps.contains(&"std".to_string()));
        assert!(deps.contains(&"soroban_sdk".to_string()));
    }

    #[test]
    fn render_ascii_not_empty() {
        let graph = CallGraph {
            root: "mycontract".to_string(),
            nodes: vec![],
            edges: vec![],
            dependencies: vec![],
            patterns: vec![],
        };
        let out = render_ascii(&graph);
        assert!(out.contains("mycontract"));
    }
}
