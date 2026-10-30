//! Multi-File Atomic Workspace Transactions & Semantic Rollback Journal.
//!
//! Provides transactional ACID-like safety for multi-file workspace modifications:
//! - Pre-mutation Write-Ahead Logging (WAL) preserving the pre-transaction baseline
//! - Atomic multi-file rollback restoring modified files, deleting created files, and restoring deleted files
//! - State persistence under `.minicode/transactions/<tx_id>/`

use crate::constants::{
    TRANSACTIONS_DIR_NAME, TRANSACTION_ACTIVE_FILE, TRANSACTION_BACKUP_DIR_NAME,
    TRANSACTION_MANIFEST_FILE, WORKSPACE_DIR_NAME,
};
use crate::error::{Result, SessionError};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Status of a workspace transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStatus {
    Active,
    Committed,
    RolledBack,
}

/// Type of filesystem operation recorded for a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    Modified,
    Created,
    Deleted,
}

/// Operation record for a single file tracked by the transaction journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOperationRecord {
    pub relative_path: String,
    pub operation: OperationType,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub backup_path: Option<String>,
    pub timestamp: String,
}

/// Persistent manifest tracking a complete multi-file transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionManifest {
    pub tx_id: String,
    pub description: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: TransactionStatus,
    pub files: Vec<FileOperationRecord>,
}

impl TransactionManifest {
    fn into_receipt(self) -> TransactionReceipt {
        let duration_secs =
            TransactionManager::elapsed_seconds(&self.started_at, self.completed_at.as_deref());
        let operations = self
            .files
            .iter()
            .map(|f| format!("[{:?}] {}", f.operation, f.relative_path))
            .collect();
        TransactionReceipt {
            tx_id: self.tx_id,
            description: self.description,
            status: self.status,
            files_count: self.files.len(),
            operations,
            started_at: self.started_at,
            completed_at: self.completed_at,
            duration_secs,
        }
    }
}

/// Pointer stored in `.minicode/transactions/active_tx.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveTxPointer {
    pub tx_id: String,
    pub started_at: String,
    pub description: String,
}

/// Receipt returned upon committing or inspecting a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionReceipt {
    pub tx_id: String,
    pub description: String,
    pub status: TransactionStatus,
    pub files_count: usize,
    pub operations: Vec<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub duration_secs: f64,
}

impl TransactionReceipt {
    pub fn format_receipt(&self) -> String {
        let mut out = String::with_capacity(512);
        out.push_str("=================================================================\n");
        out.push_str(&format!(
            "📦 WORKSPACE TRANSACTION RECEIPT: [{:?}]\n",
            self.status
        ));
        out.push_str("=================================================================\n");
        out.push_str(&format!("• Transaction ID : {}\n", self.tx_id));
        out.push_str(&format!("• Description    : {}\n", self.description));
        out.push_str(&format!("• Status         : {:?}\n", self.status));
        out.push_str(&format!("• Files Affected : {}\n", self.files_count));
        out.push_str(&format!("• Duration       : {:.2}s\n", self.duration_secs));
        out.push_str(&format!("• Started At     : {}\n", self.started_at));
        if let Some(completed) = &self.completed_at {
            out.push_str(&format!("• Completed At   : {}\n", completed));
        }

        if !self.operations.is_empty() {
            out.push_str("\nJournal Operations:\n");
            for op in &self.operations {
                out.push_str(&format!("  {}\n", op));
            }
        }
        out.push_str("=================================================================\n");
        out
    }
}

/// Receipt returned upon rolling back a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackReceipt {
    pub tx_id: String,
    pub description: String,
    pub reason: Option<String>,
    pub restored_files: Vec<String>,
    pub deleted_files: Vec<String>,
    pub duration_secs: f64,
}

impl RollbackReceipt {
    pub fn format_receipt(&self) -> String {
        let mut out = String::with_capacity(512);
        out.push_str("=================================================================\n");
        out.push_str("⏪ WORKSPACE TRANSACTION ROLLED BACK (ATOMIC RESTORE)\n");
        out.push_str("=================================================================\n");
        out.push_str(&format!("• Transaction ID : {}\n", self.tx_id));
        out.push_str(&format!("• Description    : {}\n", self.description));
        if let Some(reason) = &self.reason {
            out.push_str(&format!("• Rollback Reason: {}\n", reason));
        }
        out.push_str(&format!("• Duration       : {:.2}s\n", self.duration_secs));
        out.push_str(&format!(
            "• Restored Files : {} file(s)\n",
            self.restored_files.len()
        ));
        for file in &self.restored_files {
            out.push_str(&format!("    ✔ restored: {}\n", file));
        }
        out.push_str(&format!(
            "• Purged Files   : {} created file(s)\n",
            self.deleted_files.len()
        ));
        for file in &self.deleted_files {
            out.push_str(&format!("    ✗ purged: {}\n", file));
        }
        out.push_str("=================================================================\n");
        out
    }
}

/// Process-wide mutex serializing transaction manifest reads/writes across parallel tool executions.
static TX_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Core transaction orchestrator managing workspace-wide transactions.
pub struct TransactionManager;

impl TransactionManager {
    /// Atomically writes a transaction manifest using a temp file + rename to prevent torn writes.
    fn write_manifest_atomic(manifest_path: &Path, manifest: &TransactionManifest) -> Result<()> {
        crate::session::write_json_atomic(manifest_path, manifest)
    }

    /// Reads and parses a transaction manifest, self-healing trailing `}` characters if a legacy
    /// torn write occurred.
    fn read_manifest_lenient(manifest_path: &Path) -> Result<TransactionManifest> {
        let manifest_data = std::fs::read_to_string(manifest_path)?;
        crate::session::parse_json_lenient(&manifest_data, Some(manifest_path))
    }

    /// Directory storing all transaction states (.minicode/transactions)
    pub fn transactions_dir(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(WORKSPACE_DIR_NAME)
            .join(TRANSACTIONS_DIR_NAME)
    }

    /// Path to pointer file tracking the currently active transaction
    pub fn active_pointer_file(workspace_root: &Path) -> PathBuf {
        Self::transactions_dir(workspace_root).join(TRANSACTION_ACTIVE_FILE)
    }

    /// Directory for a specific transaction
    pub fn tx_dir(workspace_root: &Path, tx_id: &str) -> PathBuf {
        Self::transactions_dir(workspace_root).join(tx_id)
    }

    /// Computes 64-bit hex hash of a file's contents using std DefaultHasher
    pub fn compute_file_hash(file_path: &Path) -> Option<String> {
        if !file_path.exists() || !file_path.is_file() {
            return None;
        }
        match std::fs::read(file_path) {
            Ok(bytes) => Some(Self::compute_bytes_hash(&bytes)),
            Err(_) => None,
        }
    }

    /// Computes 64-bit hex hash of byte buffer
    pub fn compute_bytes_hash(bytes: &[u8]) -> String {
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Calculates elapsed seconds between an RFC3339 timestamp and now (or end timestamp)
    fn elapsed_seconds(start_rfc3339: &str, end_rfc3339: Option<&str>) -> f64 {
        let start = chrono::DateTime::parse_from_rfc3339(start_rfc3339)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());

        let end = match end_rfc3339 {
            Some(end_str) => chrono::DateTime::parse_from_rfc3339(end_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            None => chrono::Utc::now(),
        };

        (end - start).num_milliseconds().max(0) as f64 / 1000.0
    }

    /// Begins a new atomic workspace transaction.
    ///
    /// Rejects if another transaction is currently active.
    pub fn begin(workspace_root: &Path, description: &str) -> Result<TransactionManifest> {
        let _guard = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let txs_dir = Self::transactions_dir(workspace_root);
        std::fs::create_dir_all(&txs_dir)?;

        // Check if active transaction exists
        if let Some(active) = Self::get_active_unlocked(workspace_root)? {
            return Err(SessionError::TransactionActive(format!(
                "Transaction '{}' is already active: '{}'. Commit or rollback before beginning a new transaction.",
                active.tx_id, active.description
            )).into());
        }

        let timestamp_slug = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let uuid_slug = &uuid::Uuid::new_v4().to_string()[..8];
        let tx_id = format!("tx_{}_{}", timestamp_slug, uuid_slug);

        let specific_dir = Self::tx_dir(workspace_root, &tx_id);
        let backup_dir = specific_dir.join(TRANSACTION_BACKUP_DIR_NAME);
        std::fs::create_dir_all(&backup_dir)?;

        let started_at = chrono::Utc::now().to_rfc3339();
        let manifest = TransactionManifest {
            tx_id: tx_id.clone(),
            description: description.trim().to_string(),
            started_at: started_at.clone(),
            completed_at: None,
            status: TransactionStatus::Active,
            files: Vec::new(),
        };

        // Write manifest.json atomically
        let manifest_path = specific_dir.join(TRANSACTION_MANIFEST_FILE);
        Self::write_manifest_atomic(&manifest_path, &manifest)?;

        // Write active pointer
        let pointer = ActiveTxPointer {
            tx_id: tx_id.clone(),
            started_at,
            description: description.trim().to_string(),
        };
        let pointer_json = serde_json::to_string_pretty(&pointer)?;
        std::fs::write(Self::active_pointer_file(workspace_root), pointer_json)?;

        tracing::info!(
            tx_id = %tx_id,
            description = %description,
            "Began atomic workspace transaction"
        );

        Ok(manifest)
    }

    /// Loads the currently active transaction, if any.
    pub fn get_active(workspace_root: &Path) -> Result<Option<TransactionManifest>> {
        let _guard = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        Self::get_active_unlocked(workspace_root)
    }

    /// Returns true if `file_path` is already recorded in the currently active transaction.
    pub fn is_file_tracked_in_active_tx(workspace_root: &Path, file_path: &Path) -> bool {
        let _guard = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let manifest = match Self::get_active_unlocked(workspace_root) {
            Ok(Some(m)) => m,
            _ => return false,
        };
        let (_, rel_str) = match Self::normalize_rel_path(workspace_root, file_path) {
            Ok(pair) => pair,
            Err(_) => return false,
        };
        manifest.files.iter().any(|f| f.relative_path == rel_str)
    }

    fn get_active_unlocked(workspace_root: &Path) -> Result<Option<TransactionManifest>> {
        let pointer_path = Self::active_pointer_file(workspace_root);
        if !pointer_path.exists() {
            return Ok(None);
        }

        let data = match std::fs::read_to_string(&pointer_path) {
            Ok(d) => d,
            Err(_) => return Ok(None),
        };

        let pointer: ActiveTxPointer = match serde_json::from_str(&data) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "Corrupted active transaction pointer, removing");
                let _ = std::fs::remove_file(&pointer_path);
                return Ok(None);
            }
        };

        let manifest_path =
            Self::tx_dir(workspace_root, &pointer.tx_id).join(TRANSACTION_MANIFEST_FILE);
        if !manifest_path.exists() {
            tracing::warn!(
                tx_id = %pointer.tx_id,
                "Manifest not found for active transaction pointer, cleaning pointer"
            );
            let _ = std::fs::remove_file(&pointer_path);
            return Ok(None);
        }

        let manifest = Self::read_manifest_lenient(&manifest_path)?;

        if manifest.status != TransactionStatus::Active {
            // Pointer points to completed/rolled-back transaction
            let _ = std::fs::remove_file(&pointer_path);
            return Ok(None);
        }

        Ok(Some(manifest))
    }

    /// Helper to convert path to normalized relative path within workspace.
    fn normalize_rel_path(workspace_root: &Path, file_path: &Path) -> Result<(PathBuf, String)> {
        let full_file =
            crate::sandbox::path::validate_path_in_workspace(workspace_root, file_path)?;
        let canonical_ws =
            std::fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());

        let rel = full_file
            .strip_prefix(&canonical_ws)
            .or_else(|_| full_file.strip_prefix(workspace_root))
            .map_err(|_| {
                SessionError::WriteCheckpoint(format!(
                    "Path '{}' escapes workspace boundary '{}'",
                    file_path.display(),
                    workspace_root.display()
                ))
            })?;

        let rel_str = rel.to_string_lossy().into_owned();
        Ok((full_file, rel_str))
    }

    /// Pre-mutation hook: records the initial state of a file before modification.
    ///
    /// If an active transaction exists and the file has not yet been logged in the transaction,
    /// captures the baseline snapshot so multi-edit modifications can be rolled back to the start.
    pub fn record_mutation_pre(workspace_root: &Path, file_path: &Path) -> Result<()> {
        let _guard = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut manifest = match Self::get_active_unlocked(workspace_root)? {
            Some(m) => m,
            None => return Ok(()),
        };

        let (full_file, rel_str) = Self::normalize_rel_path(workspace_root, file_path)?;

        // If file is already tracked in this transaction, DO NOT overwrite the baseline backup!
        if manifest.files.iter().any(|f| f.relative_path == rel_str) {
            tracing::debug!(path = %rel_str, "File already tracked in active transaction, retaining original baseline");
            return Ok(());
        }

        let specific_dir = Self::tx_dir(workspace_root, &manifest.tx_id);
        let backup_base = specific_dir.join(TRANSACTION_BACKUP_DIR_NAME);

        let now = chrono::Utc::now().to_rfc3339();

        if full_file.exists() && full_file.is_file() {
            let before_hash = Self::compute_file_hash(&full_file);
            let backup_dest = backup_base.join(&rel_str);
            if let Some(parent) = backup_dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&full_file, &backup_dest).map_err(|e| {
                SessionError::WriteCheckpoint(format!(
                    "Failed to snapshot file {} to {}: {}",
                    full_file.display(),
                    backup_dest.display(),
                    e
                ))
            })?;

            let backup_rel = backup_dest
                .strip_prefix(&specific_dir)
                .unwrap_or(&backup_dest)
                .to_string_lossy()
                .into_owned();

            manifest.files.push(FileOperationRecord {
                relative_path: rel_str.clone(),
                operation: OperationType::Modified,
                before_hash,
                after_hash: None,
                backup_path: Some(backup_rel),
                timestamp: now,
            });

            tracing::debug!(path = %rel_str, "Recorded existing file modification baseline in transaction");
        } else {
            // File does not exist prior to modification — it will be a newly created file
            manifest.files.push(FileOperationRecord {
                relative_path: rel_str.clone(),
                operation: OperationType::Created,
                before_hash: None,
                after_hash: None,
                backup_path: None,
                timestamp: now,
            });

            tracing::debug!(path = %rel_str, "Recorded new file creation in transaction journal");
        }

        // Save updated manifest atomically
        let manifest_path = specific_dir.join(TRANSACTION_MANIFEST_FILE);
        Self::write_manifest_atomic(&manifest_path, &manifest)?;

        Ok(())
    }

    /// Post-mutation hook: computes after-hash for files modified in the active transaction.
    pub fn record_mutation_post(workspace_root: &Path, file_path: &Path) -> Result<()> {
        let _guard = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut manifest = match Self::get_active_unlocked(workspace_root)? {
            Some(m) => m,
            None => return Ok(()),
        };

        let (full_file, rel_str) = match Self::normalize_rel_path(workspace_root, file_path) {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to normalize path for post-mutation hook");
                return Ok(());
            }
        };

        if let Some(record) = manifest
            .files
            .iter_mut()
            .find(|f| f.relative_path == rel_str)
        {
            if full_file.exists() && full_file.is_file() {
                record.after_hash = Self::compute_file_hash(&full_file);
            } else if record.operation == OperationType::Modified {
                record.operation = OperationType::Deleted;
                record.after_hash = None;
            }

            let specific_dir = Self::tx_dir(workspace_root, &manifest.tx_id);
            let manifest_path = specific_dir.join(TRANSACTION_MANIFEST_FILE);
            Self::write_manifest_atomic(&manifest_path, &manifest)?;
        }

        Ok(())
    }

    fn resolve_active_manifest_unlocked(
        workspace_root: &Path,
        tx_id: Option<&str>,
        verb: &str,
    ) -> Result<(TransactionManifest, PathBuf, PathBuf)> {
        let manifest = match tx_id {
            Some(id) => {
                let specific_dir = Self::tx_dir(workspace_root, id);
                let manifest_path = specific_dir.join(TRANSACTION_MANIFEST_FILE);
                if !manifest_path.exists() {
                    return Err(SessionError::NotFound {
                        id: id.to_string(),
                        path: manifest_path.to_string_lossy().into_owned(),
                    }
                    .into());
                }
                Self::read_manifest_lenient(&manifest_path)?
            }
            None => Self::get_active_unlocked(workspace_root)?
                .ok_or(SessionError::NoActiveTransaction)?,
        };

        if manifest.status != TransactionStatus::Active {
            return Err(SessionError::Transaction(format!(
                "Transaction '{}' cannot be {} because its status is '{:?}'",
                manifest.tx_id, verb, manifest.status
            ))
            .into());
        }

        let specific_dir = Self::tx_dir(workspace_root, &manifest.tx_id);
        let manifest_path = specific_dir.join(TRANSACTION_MANIFEST_FILE);
        Ok((manifest, specific_dir, manifest_path))
    }

    fn clear_active_pointer_if_matches(workspace_root: &Path, target_tx_id: &str) {
        let pointer_path = Self::active_pointer_file(workspace_root);
        if let Ok(data) = std::fs::read_to_string(&pointer_path) {
            if let Ok(pointer) = serde_json::from_str::<ActiveTxPointer>(&data) {
                if pointer.tx_id == target_tx_id {
                    let _ = std::fs::remove_file(&pointer_path);
                }
            }
        }
    }

    /// Commits an active transaction, finalizing its status and sealing the journal.
    pub fn commit(workspace_root: &Path, tx_id: Option<&str>) -> Result<TransactionReceipt> {
        let _guard = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut manifest, _specific_dir, manifest_path) =
            Self::resolve_active_manifest_unlocked(workspace_root, tx_id, "committed")?;

        let completed_at = chrono::Utc::now().to_rfc3339();
        manifest.status = TransactionStatus::Committed;
        manifest.completed_at = Some(completed_at);

        // Refresh after-hashes for modified/created files
        for record in &mut manifest.files {
            let full_file = workspace_root.join(&record.relative_path);
            if full_file.exists() && full_file.is_file() {
                record.after_hash = Self::compute_file_hash(&full_file);
            }
        }

        Self::write_manifest_atomic(&manifest_path, &manifest)?;

        // Ensure .minicode/ directory remains protected in .gitignore after any file scaffolding or commits
        crate::tools::minikit::sync::MiniKitSyncEngine::ensure_gitignore(workspace_root);

        Self::clear_active_pointer_if_matches(workspace_root, &manifest.tx_id);

        let receipt = manifest.into_receipt();
        tracing::info!(
            tx_id = %receipt.tx_id,
            files_count = receipt.files_count,
            duration_secs = receipt.duration_secs,
            "Committed workspace transaction successfully"
        );

        Ok(receipt)
    }

    /// Atomically rolls back a transaction, reverting modified files and removing created files.
    pub fn rollback(
        workspace_root: &Path,
        tx_id: Option<&str>,
        reason: Option<&str>,
    ) -> Result<RollbackReceipt> {
        let _guard = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut manifest, specific_dir, manifest_path) =
            Self::resolve_active_manifest_unlocked(workspace_root, tx_id, "rolled back")?;

        let mut restored_files = Vec::new();
        let mut deleted_files = Vec::new();

        // Roll back in reverse chronological order
        for record in manifest.files.iter().rev() {
            let target_path = workspace_root.join(&record.relative_path);

            // Defense in depth: validate boundary
            if let Err(e) =
                crate::sandbox::path::validate_path_in_workspace(workspace_root, &target_path)
            {
                tracing::warn!(
                    path = %record.relative_path,
                    error = %e,
                    "Skipping path escaping workspace boundary during transaction rollback"
                );
                continue;
            }

            match record.operation {
                OperationType::Modified | OperationType::Deleted => {
                    if let Some(backup_rel) = &record.backup_path {
                        let backup_file = specific_dir.join(backup_rel);
                        if backup_file.exists() {
                            if let Some(parent) = target_path.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            std::fs::copy(&backup_file, &target_path).map_err(|e| {
                                SessionError::WriteCheckpoint(format!(
                                    "Failed to restore {} from {}: {}",
                                    target_path.display(),
                                    backup_file.display(),
                                    e
                                ))
                            })?;
                            restored_files.push(record.relative_path.clone());
                            tracing::info!(
                                path = %record.relative_path,
                                op = ?record.operation,
                                "Restored file to pre-transaction state"
                            );
                        }
                    }
                }
                OperationType::Created => {
                    if target_path.exists() {
                        let remove_res = if target_path.is_dir() {
                            std::fs::remove_dir_all(&target_path)
                        } else {
                            std::fs::remove_file(&target_path)
                        };
                        if let Err(e) = remove_res {
                            tracing::warn!(
                                path = %record.relative_path,
                                error = %e,
                                "Failed to remove newly created file during transaction rollback"
                            );
                        } else {
                            deleted_files.push(record.relative_path.clone());
                            tracing::info!(path = %record.relative_path, "Purged file created during rolled-back transaction");
                        }
                    }
                }
            }
        }

        let completed_at = chrono::Utc::now().to_rfc3339();
        manifest.status = TransactionStatus::RolledBack;
        manifest.completed_at = Some(completed_at.clone());

        Self::write_manifest_atomic(&manifest_path, &manifest)?;

        Self::clear_active_pointer_if_matches(workspace_root, &manifest.tx_id);

        let duration_secs = Self::elapsed_seconds(&manifest.started_at, Some(&completed_at));

        tracing::info!(
            tx_id = %manifest.tx_id,
            restored = restored_files.len(),
            purged = deleted_files.len(),
            "Successfully rolled back transaction"
        );

        Ok(RollbackReceipt {
            tx_id: manifest.tx_id,
            description: manifest.description,
            reason: reason.map(str::to_string),
            restored_files,
            deleted_files,
            duration_secs,
        })
    }

    /// Gets status of active transaction, or of a specific transaction ID.
    pub fn status(
        workspace_root: &Path,
        tx_id: Option<&str>,
    ) -> Result<Option<TransactionReceipt>> {
        let _guard = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let manifest = match tx_id {
            Some(id) => {
                let specific_dir = Self::tx_dir(workspace_root, id);
                let manifest_path = specific_dir.join(TRANSACTION_MANIFEST_FILE);
                if !manifest_path.exists() {
                    return Ok(None);
                }
                Self::read_manifest_lenient(&manifest_path).map(Some)?
            }
            None => Self::get_active_unlocked(workspace_root)?,
        };

        Ok(manifest.map(TransactionManifest::into_receipt))
    }

    /// Lists all transactions recorded in this workspace, sorted newest first.
    #[allow(dead_code)]
    pub fn list(workspace_root: &Path) -> Result<Vec<TransactionManifest>> {
        let _guard = TX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let txs_dir = Self::transactions_dir(workspace_root);
        if !txs_dir.exists() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        for entry in std::fs::read_dir(txs_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let manifest_path = entry.path().join(TRANSACTION_MANIFEST_FILE);
                if manifest_path.exists() {
                    if let Ok(manifest) = Self::read_manifest_lenient(&manifest_path) {
                        out.push(manifest);
                    }
                }
            }
        }

        out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_begin_and_get_active_transaction() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        assert!(TransactionManager::get_active(ws).unwrap().is_none());

        let manifest = TransactionManager::begin(ws, "Test refactoring").unwrap();
        assert_eq!(manifest.status, TransactionStatus::Active);
        assert_eq!(manifest.description, "Test refactoring");
        assert!(manifest.tx_id.starts_with("tx_"));

        let active = TransactionManager::get_active(ws).unwrap().unwrap();
        assert_eq!(active.tx_id, manifest.tx_id);
        assert_eq!(active.status, TransactionStatus::Active);

        // Cannot begin duplicate transaction while active
        let err = TransactionManager::begin(ws, "Another refactor");
        assert!(err.is_err());
    }

    #[test]
    fn test_transaction_multi_file_mutation_and_commit() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        let file_a = ws.join("file_a.txt");
        std::fs::write(&file_a, "initial_a").unwrap();

        TransactionManager::begin(ws, "Modify file A and create file B").unwrap();

        // Hook pre-mutation on existing file A
        TransactionManager::record_mutation_pre(ws, &file_a).unwrap();
        std::fs::write(&file_a, "modified_a").unwrap();
        TransactionManager::record_mutation_post(ws, &file_a).unwrap();

        // Hook pre-mutation on new file B
        let file_b = ws.join("file_b.txt");
        TransactionManager::record_mutation_pre(ws, &file_b).unwrap();
        std::fs::write(&file_b, "created_b").unwrap();
        TransactionManager::record_mutation_post(ws, &file_b).unwrap();

        // Verify status
        let status = TransactionManager::status(ws, None).unwrap().unwrap();
        assert_eq!(status.files_count, 2);
        assert_eq!(status.status, TransactionStatus::Active);

        // Commit transaction
        let receipt = TransactionManager::commit(ws, None).unwrap();
        assert_eq!(receipt.status, TransactionStatus::Committed);
        assert_eq!(receipt.files_count, 2);

        // Active pointer removed
        assert!(TransactionManager::get_active(ws).unwrap().is_none());

        // File contents remain committed
        assert_eq!(std::fs::read_to_string(&file_a).unwrap(), "modified_a");
        assert_eq!(std::fs::read_to_string(&file_b).unwrap(), "created_b");
    }

    #[test]
    fn test_transaction_atomic_rollback() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        let file_a = ws.join("src").join("mod_a.rs");
        std::fs::create_dir_all(file_a.parent().unwrap()).unwrap();
        std::fs::write(&file_a, "fn original_code() {}").unwrap();

        TransactionManager::begin(ws, "High stakes refactor to be rolled back").unwrap();

        // Hook pre-mutation on file_a
        TransactionManager::record_mutation_pre(ws, &file_a).unwrap();
        std::fs::write(&file_a, "fn broken_code() {}").unwrap();
        TransactionManager::record_mutation_post(ws, &file_a).unwrap();

        // Second edit on same file in same transaction: must NOT overwrite baseline
        TransactionManager::record_mutation_pre(ws, &file_a).unwrap();
        std::fs::write(&file_a, "fn broken_code_v2() {}").unwrap();
        TransactionManager::record_mutation_post(ws, &file_a).unwrap();

        // Hook pre-mutation on newly created file_b
        let file_b = ws.join("src").join("new_file.rs");
        TransactionManager::record_mutation_pre(ws, &file_b).unwrap();
        std::fs::write(&file_b, "pub struct NewType;").unwrap();
        TransactionManager::record_mutation_post(ws, &file_b).unwrap();

        assert_eq!(
            std::fs::read_to_string(&file_a).unwrap(),
            "fn broken_code_v2() {}"
        );
        assert!(file_b.exists());

        // Execute rollback
        let rollback_receipt =
            TransactionManager::rollback(ws, None, Some("Compiler verification failed on Gate 1"))
                .unwrap();

        assert_eq!(rollback_receipt.restored_files.len(), 1);
        assert_eq!(rollback_receipt.deleted_files.len(), 1);

        // File A restored to ORIGINAL baseline
        assert_eq!(
            std::fs::read_to_string(&file_a).unwrap(),
            "fn original_code() {}"
        );
        // File B purged
        assert!(!file_b.exists());

        // Active pointer cleared
        assert!(TransactionManager::get_active(ws).unwrap().is_none());
    }

    #[test]
    fn test_transaction_concurrent_mutation_hooks_and_self_healing() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        let tx = TransactionManager::begin(ws, "Parallel write_file batch").unwrap();

        // Simulate 8 parallel tool workers recording pre/post mutations simultaneously
        std::thread::scope(|s| {
            for i in 0..8 {
                s.spawn(move || {
                    let f = ws.join(format!("parallel_{}.tsx", i));
                    TransactionManager::record_mutation_pre(ws, &f).unwrap();
                    std::fs::write(&f, format!("export const C{} = () => null;", i)).unwrap();
                    TransactionManager::record_mutation_post(ws, &f).unwrap();
                });
            }
        });

        let active = TransactionManager::get_active(ws).unwrap().unwrap();
        assert_eq!(active.files.len(), 8);

        // Simulate a legacy torn write with trailing `}}`
        let manifest_path = TransactionManager::tx_dir(ws, &tx.tx_id).join("manifest.json");
        let raw = std::fs::read_to_string(&manifest_path).unwrap();
        std::fs::write(&manifest_path, format!("{}\n}}", raw)).unwrap();

        // get_active should self-heal the trailing brace without failing
        let healed = TransactionManager::get_active(ws).unwrap().unwrap();
        assert_eq!(healed.files.len(), 8);
        assert!(TransactionManager::is_file_tracked_in_active_tx(
            ws,
            &ws.join("parallel_3.tsx")
        ));
    }
}
