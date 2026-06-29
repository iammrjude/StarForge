use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeployStatus {
    Success,
    Failed,
    RolledBack,
    Pending,
}

impl std::fmt::Display for DeployStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeployStatus::Success => write!(f, "success"),
            DeployStatus::Failed => write!(f, "failed"),
            DeployStatus::RolledBack => write!(f, "rolled-back"),
            DeployStatus::Pending => write!(f, "pending"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRecord {
    pub id: String,
    pub contract_id: Option<String>,
    pub wasm_path: String,
    pub wasm_hash: String,
    pub network: String,
    pub wallet: String,
    pub timestamp: String,
    pub status: DeployStatus,
    pub error: Option<String>,
    pub previous_id: Option<String>,
    pub approved_by: Option<String>,
    pub verification_passed: bool,
}

impl DeployRecord {
    pub fn new(
        wasm_path: &str,
        wasm_hash: &str,
        network: &str,
        wallet: &str,
        previous_id: Option<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            contract_id: None,
            wasm_path: wasm_path.to_string(),
            wasm_hash: wasm_hash.to_string(),
            network: network.to_string(),
            wallet: wallet.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            status: DeployStatus::Pending,
            error: None,
            previous_id,
            approved_by: None,
            verification_passed: false,
        }
    }
}

fn history_path() -> PathBuf {
    crate::utils::config::config_dir().join("deploy_history.json")
}

pub fn load_history() -> Result<Vec<DeployRecord>> {
    let path = history_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data).unwrap_or_default())
}

pub fn save_history(records: &[DeployRecord]) -> Result<()> {
    let path = history_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(records)?;
    fs::write(&path, data)?;
    Ok(())
}

pub fn record_deployment(record: DeployRecord) -> Result<String> {
    let mut history = load_history()?;
    let id = record.id.clone();
    history.push(record);
    save_history(&history)?;
    Ok(id)
}

pub fn update_status(id: &str, status: DeployStatus, error: Option<String>) -> Result<()> {
    let mut history = load_history()?;
    if let Some(rec) = history.iter_mut().find(|r| r.id == id) {
        rec.status = status;
        rec.error = error;
    }
    save_history(&history)
}

pub fn set_contract_id(id: &str, contract_id: &str) -> Result<()> {
    let mut history = load_history()?;
    if let Some(rec) = history.iter_mut().find(|r| r.id == id) {
        rec.contract_id = Some(contract_id.to_string());
    }
    save_history(&history)
}

pub fn set_verified(id: &str, passed: bool) -> Result<()> {
    let mut history = load_history()?;
    if let Some(rec) = history.iter_mut().find(|r| r.id == id) {
        rec.verification_passed = passed;
    }
    save_history(&history)
}

pub fn get_record(id: &str) -> Result<Option<DeployRecord>> {
    let history = load_history()?;
    Ok(history
        .into_iter()
        .find(|r| r.id == id || r.id.starts_with(id)))
}

pub fn last_successful(network: &str) -> Result<Option<DeployRecord>> {
    let history = load_history()?;
    Ok(history
        .into_iter()
        .rev()
        .find(|r| r.network == network && r.status == DeployStatus::Success))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_record_new_has_pending_status() {
        let r = DeployRecord::new("a.wasm", "abc123", "testnet", "alice", None);
        assert_eq!(r.status, DeployStatus::Pending);
        assert!(r.contract_id.is_none());
    }

    #[test]
    fn deploy_status_display() {
        assert_eq!(DeployStatus::Success.to_string(), "success");
        assert_eq!(DeployStatus::RolledBack.to_string(), "rolled-back");
    }
}
