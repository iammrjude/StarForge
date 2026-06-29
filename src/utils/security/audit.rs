use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityFinding {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub description: String,
    pub location: Option<String>,
    pub tool: String,
    pub remediation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSummary {
    pub critical: u32,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
    pub info: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResult {
    pub timestamp: String,
    pub contract_path: String,
    pub score: f64,
    pub findings: Vec<VulnerabilityFinding>,
    pub tools_used: Vec<String>,
    pub summary: AuditSummary,
}

pub struct AuditConfig {
    pub run_slither: bool,
    pub run_mythril: bool,
}

pub fn run_audit(path: &Path, config: &AuditConfig) -> Result<AuditResult> {
    let mut findings = Vec::new();
    let mut tools_used = Vec::new();

    let builtin = run_builtin_analysis(path)?;
    findings.extend(builtin);
    tools_used.push("starforge-builtin".to_string());

    if config.run_slither && is_tool_available("slither") {
        match run_slither(path) {
            Ok(mut sf) => {
                findings.append(&mut sf);
                tools_used.push("slither".to_string());
            }
            Err(e) => eprintln!("Warning: Slither failed: {}", e),
        }
    }

    if config.run_mythril && is_tool_available("myth") {
        match run_mythril(path) {
            Ok(mut mf) => {
                findings.append(&mut mf);
                tools_used.push("mythril".to_string());
            }
            Err(e) => eprintln!("Warning: Mythril failed: {}", e),
        }
    }

    let summary = compute_summary(&findings);
    let score = compute_score(&summary);

    Ok(AuditResult {
        timestamp: Utc::now().to_rfc3339(),
        contract_path: path.to_string_lossy().to_string(),
        score,
        findings,
        tools_used,
        summary,
    })
}

fn is_tool_available(tool: &str) -> bool {
    Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_slither(path: &Path) -> Result<Vec<VulnerabilityFinding>> {
    let output = Command::new("slither")
        .arg(path)
        .arg("--json")
        .arg("-")
        .output()?;

    let json_str = String::from_utf8_lossy(&output.stdout);
    parse_slither_output(&json_str)
}

fn parse_slither_output(json_str: &str) -> Result<Vec<VulnerabilityFinding>> {
    #[derive(Deserialize)]
    struct SlitherOut {
        results: Option<SlitherDetectors>,
    }
    #[derive(Deserialize)]
    struct SlitherDetectors {
        detectors: Option<Vec<SlitherDet>>,
    }
    #[derive(Deserialize)]
    struct SlitherDet {
        check: String,
        impact: String,
        description: String,
        elements: Option<Vec<SlitherElem>>,
    }
    #[derive(Deserialize)]
    struct SlitherElem {
        source_mapping: Option<SlitherSrc>,
    }
    #[derive(Deserialize)]
    struct SlitherSrc {
        filename_used: Option<String>,
        lines: Option<Vec<u32>>,
    }

    let result: SlitherOut = serde_json::from_str(json_str).unwrap_or(SlitherOut { results: None });
    let mut findings = Vec::new();

    if let Some(detectors) = result.results.and_then(|r| r.detectors) {
        for det in detectors {
            let severity = match det.impact.as_str() {
                "High" => "high",
                "Medium" => "medium",
                "Low" => "low",
                _ => "info",
            };
            let location = det
                .elements
                .as_ref()
                .and_then(|els| els.first())
                .and_then(|el| el.source_mapping.as_ref())
                .map(|sm| {
                    let file = sm.filename_used.as_deref().unwrap_or("unknown");
                    let lines = sm.lines.as_deref().unwrap_or(&[]);
                    match (lines.first(), lines.last()) {
                        (Some(f), Some(l)) => format!("{}:{}-{}", file, f, l),
                        _ => file.to_string(),
                    }
                });
            findings.push(VulnerabilityFinding {
                id: format!("SLITHER-{}", det.check.to_uppercase().replace('-', "_")),
                title: det.check.clone(),
                severity: severity.to_string(),
                description: det.description.clone(),
                location,
                tool: "slither".to_string(),
                remediation: slither_remediation(&det.check),
            });
        }
    }
    Ok(findings)
}

fn slither_remediation(check: &str) -> String {
    match check {
        "reentrancy-eth" | "reentrancy-no-eth" => {
            "Use the checks-effects-interactions pattern or a reentrancy guard.".to_string()
        }
        "uninitialized-state" | "uninitialized-storage" => {
            "Initialize all state variables before use.".to_string()
        }
        "integer-overflow" | "integer-underflow" => {
            "Use checked arithmetic operations.".to_string()
        }
        "arbitrary-send-eth" => "Validate the recipient address before sending funds.".to_string(),
        "suicidal" => "Remove or restrict access to self-destruct functionality.".to_string(),
        _ => format!("Review and fix the '{}' vulnerability pattern.", check),
    }
}

fn run_mythril(path: &Path) -> Result<Vec<VulnerabilityFinding>> {
    let output = Command::new("myth")
        .arg("analyze")
        .arg(path)
        .arg("--output")
        .arg("json")
        .output()?;

    let json_str = String::from_utf8_lossy(&output.stdout);
    parse_mythril_output(&json_str)
}

fn parse_mythril_output(json_str: &str) -> Result<Vec<VulnerabilityFinding>> {
    #[derive(Deserialize)]
    struct MythReport {
        issues: Option<Vec<MythIssue>>,
    }
    #[derive(Deserialize)]
    struct MythIssue {
        title: String,
        severity: String,
        description: Option<MythDesc>,
        filename: Option<String>,
        lineno: Option<u32>,
    }
    #[derive(Deserialize)]
    struct MythDesc {
        head: String,
        tail: Option<String>,
    }

    let report: MythReport = serde_json::from_str(json_str).unwrap_or(MythReport { issues: None });
    let mut findings = Vec::new();

    for issue in report.issues.unwrap_or_default() {
        let description = issue
            .description
            .as_ref()
            .map(|d| format!("{} {}", d.head, d.tail.as_deref().unwrap_or("")))
            .unwrap_or_else(|| issue.title.clone());

        let location = match (&issue.filename, issue.lineno) {
            (Some(f), Some(l)) => Some(format!("{}:{}", f, l)),
            (Some(f), None) => Some(f.clone()),
            _ => None,
        };

        let severity = match issue.severity.as_str() {
            "High" => "high",
            "Medium" => "medium",
            "Low" => "low",
            _ => "info",
        };

        findings.push(VulnerabilityFinding {
            id: format!("MYTHRIL-{}", issue.title.to_uppercase().replace(' ', "_")),
            title: issue.title.clone(),
            severity: severity.to_string(),
            description,
            location,
            tool: "mythril".to_string(),
            remediation: "Review the Mythril finding and apply the recommended fix.".to_string(),
        });
    }
    Ok(findings)
}

fn run_builtin_analysis(path: &Path) -> Result<Vec<VulnerabilityFinding>> {
    let result = super::checklist::run_checklist(path)?;
    let mut findings = Vec::new();

    for item in result.items {
        if !item.passed {
            findings.push(VulnerabilityFinding {
                id: format!("SF-{}", item.id.to_uppercase()),
                title: item.title.clone(),
                severity: item.severity.clone(),
                description: item.description.clone(),
                location: Some(path.to_string_lossy().to_string()),
                tool: "starforge-builtin".to_string(),
                remediation: builtin_remediation(&item.id),
            });
        }
    }
    Ok(findings)
}

fn builtin_remediation(id: &str) -> String {
    match id {
        "auth_check" => {
            "Add authorization checks using require_auth() before sensitive operations.".to_string()
        }
        "overflow" => {
            "Use checked arithmetic operations (checked_add, checked_sub, etc.).".to_string()
        }
        "panic" => {
            "Replace expect()/unwrap() with proper error handling using Result<T, E>.".to_string()
        }
        "reentrancy" => {
            "Avoid calling external contracts mid-function; emit events after state changes."
                .to_string()
        }
        _ => format!("Review and fix the '{}' security pattern.", id),
    }
}

fn compute_summary(findings: &[VulnerabilityFinding]) -> AuditSummary {
    let mut s = AuditSummary {
        critical: 0,
        high: 0,
        medium: 0,
        low: 0,
        info: 0,
    };
    for f in findings {
        match f.severity.as_str() {
            "critical" => s.critical += 1,
            "high" => s.high += 1,
            "medium" => s.medium += 1,
            "low" => s.low += 1,
            _ => s.info += 1,
        }
    }
    s
}

fn compute_score(s: &AuditSummary) -> f64 {
    let penalty = (s.critical as f64 * 30.0)
        + (s.high as f64 * 15.0)
        + (s.medium as f64 * 7.5)
        + (s.low as f64 * 2.5)
        + (s.info as f64 * 0.5);
    (100.0 - penalty).max(0.0)
}

pub fn format_report(result: &AuditResult) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Security Audit Report\n\
         =====================\n\
         Contract : {}\n\
         Timestamp: {}\n\
         Tools    : {}\n\
         Score    : {:.1}/100\n\n",
        result.contract_path,
        result.timestamp,
        result.tools_used.join(", "),
        result.score,
    ));
    out.push_str(&format!(
        "Summary\n\
         -------\n\
         Critical : {}\n\
         High     : {}\n\
         Medium   : {}\n\
         Low      : {}\n\
         Info     : {}\n\n",
        result.summary.critical,
        result.summary.high,
        result.summary.medium,
        result.summary.low,
        result.summary.info,
    ));
    if result.findings.is_empty() {
        out.push_str("No issues found.\n");
    } else {
        out.push_str("Findings\n--------\n");
        for (i, f) in result.findings.iter().enumerate() {
            out.push_str(&format!(
                "{}. [{}] {} ({})\n   {}\n   Remediation: {}\n",
                i + 1,
                f.severity.to_uppercase(),
                f.title,
                f.tool,
                f.description,
                f.remediation,
            ));
            if let Some(loc) = &f.location {
                out.push_str(&format!("   Location: {}\n", loc));
            }
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_full_for_no_findings() {
        let s = AuditSummary {
            critical: 0,
            high: 0,
            medium: 0,
            low: 0,
            info: 0,
        };
        assert_eq!(compute_score(&s), 100.0);
    }

    #[test]
    fn score_floored_at_zero() {
        let s = AuditSummary {
            critical: 10,
            high: 10,
            medium: 10,
            low: 10,
            info: 10,
        };
        assert_eq!(compute_score(&s), 0.0);
    }

    #[test]
    fn summary_counts_correctly() {
        let findings = vec![
            VulnerabilityFinding {
                id: "x".to_string(),
                title: "t".to_string(),
                severity: "high".to_string(),
                description: "d".to_string(),
                location: None,
                tool: "builtin".to_string(),
                remediation: "r".to_string(),
            },
            VulnerabilityFinding {
                id: "y".to_string(),
                title: "t2".to_string(),
                severity: "low".to_string(),
                description: "d2".to_string(),
                location: None,
                tool: "builtin".to_string(),
                remediation: "r2".to_string(),
            },
        ];
        let s = compute_summary(&findings);
        assert_eq!(s.high, 1);
        assert_eq!(s.low, 1);
        assert_eq!(s.critical + s.medium + s.info, 0);
    }
}
