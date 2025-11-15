//! Multi-Version Concurrency Control
//!
//! Implements MVCC for snapshot isolation and consistent reads

use crate::{Result, TransactionId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Visibility information for a record version
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VisibilityInfo {
    /// Transaction that created this version
    pub creator_id: TransactionId,
    /// Transaction that deleted this version (None if not deleted)
    pub deleter_id: Option<TransactionId>,
    /// Commit timestamp for the creator
    pub commit_ts: u64,
    /// Commit timestamp for the deleter
    pub delete_ts: Option<u64>,
}

impl VisibilityInfo {
    /// Create new visibility info for insertion
    pub fn new(transaction_id: TransactionId, commit_ts: u64) -> Self {
        Self {
            creator_id: transaction_id,
            deleter_id: None,
            commit_ts,
            delete_ts: None,
        }
    }

    /// Mark as deleted
    pub fn delete(&mut self, transaction_id: TransactionId, delete_ts: u64) {
        self.deleter_id = Some(transaction_id);
        self.delete_ts = Some(delete_ts);
    }

    /// Check if version is visible to a transaction at a given snapshot
    pub fn is_visible(&self, transaction_id: TransactionId, snapshot_ts: u64) -> bool {
        // Version must be committed before snapshot
        if self.commit_ts > snapshot_ts {
            return false;
        }

        // Version must not be deleted before snapshot
        if let Some(delete_ts) = self.delete_ts {
            if delete_ts <= snapshot_ts {
                return false;
            }
        }

        // Can't see own modifications before they're committed
        if self.creator_id == transaction_id {
            return false;
        }

        true
    }

    /// Check if transaction can modify this version
    pub fn can_modify(&self, transaction_id: TransactionId) -> bool {
        // Can modify if not deleted by another committed transaction
        match self.deleter_id {
            Some(deleter_id) => deleter_id == transaction_id,
            None => true,
        }
    }
}

/// Record version with visibility information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordVersion {
    /// Record data
    pub data: Vec<u8>,
    /// Visibility information
    pub visibility: VisibilityInfo,
    /// Version number within the record
    pub version: u32,
    /// Size of this version
    pub size: u32,
}

impl RecordVersion {
    /// Create new record version
    pub fn new(
        data: Vec<u8>,
        transaction_id: TransactionId,
        commit_ts: u64,
        version: u32,
    ) -> Self {
        let size = data.len() as u32;
        Self {
            data,
            visibility: VisibilityInfo::new(transaction_id, commit_ts),
            version,
            size,
        }
    }
}

/// Version chain for multiple versions of a record
#[derive(Debug)]
pub struct VersionChain {
    /// All versions of this record, ordered by version number
    versions: Vec<RecordVersion>,
    /// Latest committed version
    latest_committed: Option<usize>,
}

impl VersionChain {
    /// Create new version chain
    pub fn new() -> Self {
        Self {
            versions: Vec::new(),
            latest_committed: None,
        }
    }

    /// Add a new version to the chain
    pub fn add_version(&mut self, version: RecordVersion) {
        self.versions.push(version);
        self.latest_committed = Some(self.versions.len() - 1);
    }

    /// Get visible version for a transaction at snapshot time
    pub fn get_visible_version(
        &self,
        transaction_id: TransactionId,
        snapshot_ts: u64,
    ) -> Option<&RecordVersion> {
        // Search versions in reverse order (newest first)
        for version in self.versions.iter().rev() {
            if version.visibility.is_visible(transaction_id, snapshot_ts) {
                return Some(version);
            }
        }
        None
    }

    /// Get the latest committed version
    pub fn get_latest_committed(&self) -> Option<&RecordVersion> {
        self.latest_committed.and_then(|idx| self.versions.get(idx))
    }

    /// Mark version as deleted
    pub fn delete_version(
        &mut self,
        transaction_id: TransactionId,
        delete_ts: u64,
        version_to_delete: u32,
    ) -> Result<()> {
        for version in &mut self.versions {
            if version.version == version_to_delete {
                if !version.visibility.can_modify(transaction_id) {
                    return Err(crate::error::RustgreSQLError::Transaction(
                        "Cannot delete version modified by another transaction".to_string()
                    ));
                }
                version.visibility.delete(transaction_id, delete_ts);
                return Ok(());
            }
        }
        Err(crate::error::RustgreSQLError::Transaction(
            "Version not found".to_string()
        ))
    }

    /// Get all versions for debugging/cleanup
    pub fn get_all_versions(&self) -> &[RecordVersion] {
        &self.versions
    }

    /// Check if any version is visible
    pub fn has_visible_version(&self, transaction_id: TransactionId, snapshot_ts: u64) -> bool {
        self.get_visible_version(transaction_id, snapshot_ts).is_some()
    }
}

/// Snapshot information for a transaction
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Transaction ID
    pub transaction_id: TransactionId,
    /// Snapshot timestamp
    pub timestamp: u64,
    /// Active transaction IDs at snapshot time
    pub active_transactions: std::collections::HashSet<TransactionId>,
}

impl Snapshot {
    /// Create new snapshot
    pub fn new(
        transaction_id: TransactionId,
        timestamp: u64,
        active_transactions: std::collections::HashSet<TransactionId>,
    ) -> Self {
        Self {
            transaction_id,
            timestamp,
            active_transactions,
        }
    }

    /// Check if transaction was active at snapshot time
    pub fn is_transaction_active(&self, transaction_id: TransactionId) -> bool {
        self.active_transactions.contains(&transaction_id)
    }
}

/// MVCC manager for handling version chains and visibility
#[derive(Debug)]
pub struct MVCCManager {
    /// Version chains by record ID
    version_chains: Arc<Mutex<HashMap<u64, VersionChain>>>,
    /// Global timestamp counter
    global_timestamp: Arc<Mutex<u64>>,
    /// Active transactions
    active_transactions: Arc<Mutex<std::collections::HashSet<TransactionId>>>,
}

impl MVCCManager {
    /// Create new MVCC manager
    pub fn new() -> Self {
        Self {
            version_chains: Arc::new(Mutex::new(HashMap::new())),
            global_timestamp: Arc::new(Mutex::new(1)),
            active_transactions: Arc::new(Mutex::new(std::collections::HashSet::new())),
        }
    }

    /// Begin a new transaction
    pub fn begin_transaction(&self, transaction_id: TransactionId) -> Snapshot {
        let timestamp = {
            let mut ts = self.global_timestamp.lock().unwrap();
            let current = *ts;
            *ts += 1;
            current
        };

        {
            let mut active = self.active_transactions.lock().unwrap();
            active.insert(transaction_id);
        }

        let active_transactions = {
            let active = self.active_transactions.lock().unwrap();
            active.clone()
        };

        Snapshot::new(transaction_id, timestamp, active_transactions)
    }

    /// Commit a transaction
    pub fn commit_transaction(&self, transaction_id: TransactionId, snapshot_ts: u64) -> u64 {
        let commit_ts = {
            let mut ts = self.global_timestamp.lock().unwrap();
            let current = *ts;
            *ts += 1;
            current
        };

        // Update commit timestamps for all versions created by this transaction
        {
            let mut chains = self.version_chains.lock().unwrap();
            for chain in chains.values_mut() {
                for version in &mut chain.versions {
                    if version.visibility.creator_id == transaction_id {
                        version.visibility.commit_ts = commit_ts;
                    }
                }
            }
        }

        // Remove from active transactions
        {
            let mut active = self.active_transactions.lock().unwrap();
            active.remove(&transaction_id);
        }

        commit_ts
    }

    /// Abort a transaction
    pub fn abort_transaction(&self, transaction_id: TransactionId) {
        // Remove versions created by this transaction
        {
            let mut chains = self.version_chains.lock().unwrap();
            for chain in chains.values_mut() {
                chain.versions.retain(|v| v.visibility.creator_id != transaction_id);
            }

            // Clean up empty version chains
            chains.retain(|_, chain| !chain.versions.is_empty());
        }

        // Remove from active transactions
        {
            let mut active = self.active_transactions.lock().unwrap();
            active.remove(&transaction_id);
        }
    }

    /// Insert a new record version
    pub fn insert_version(
        &self,
        record_id: u64,
        data: Vec<u8>,
        transaction_id: TransactionId,
        version: u32,
    ) -> Result<()> {
        let visibility = VisibilityInfo::new(transaction_id, 0); // Will be set on commit
        let record_version = RecordVersion::new(data, transaction_id, 0, version);

        let mut chains = self.version_chains.lock().unwrap();
        let chain = chains.entry(record_id).or_insert_with(VersionChain::new);
        chain.add_version(record_version);

        Ok(())
    }

    /// Update a record version
    pub fn update_version(
        &self,
        record_id: u64,
        data: Vec<u8>,
        transaction_id: TransactionId,
        version: u32,
    ) -> Result<()> {
        let record_version = RecordVersion::new(data, transaction_id, 0, version);

        let mut chains = self.version_chains.lock().unwrap();
        let chain = chains.entry(record_id).or_insert_with(VersionChain::new);
        chain.add_version(record_version);

        Ok(())
    }

    /// Delete a record version
    pub fn delete_version(
        &self,
        record_id: u64,
        transaction_id: TransactionId,
        version_to_delete: u32,
    ) -> Result<()> {
        let mut chains = self.version_chains.lock().unwrap();
        if let Some(chain) = chains.get_mut(&record_id) {
            let delete_ts = {
                let mut ts = self.global_timestamp.lock().unwrap();
                let current = *ts;
                *ts += 1;
                current
            };
            chain.delete_version(transaction_id, delete_ts, version_to_delete)?;
        }

        Ok(())
    }

    /// Read a record at snapshot time
    pub fn read_record(
        &self,
        record_id: u64,
        snapshot: &Snapshot,
    ) -> Result<Option<Vec<u8>>> {
        let chains = self.version_chains.lock().unwrap();
        if let Some(chain) = chains.get(&record_id) {
            if let Some(version) = chain.get_visible_version(snapshot.transaction_id, snapshot.timestamp) {
                return Ok(Some(version.data.clone()));
            }
        }
        Ok(None)
    }

    /// Check if record exists at snapshot time
    pub fn record_exists(&self, record_id: u64, snapshot: &Snapshot) -> bool {
        let chains = self.version_chains.lock().unwrap();
        if let Some(chain) = chains.get(&record_id) {
            chain.has_visible_version(snapshot.transaction_id, snapshot.timestamp)
        } else {
            false
        }
    }

    /// Get the latest committed version (for non-MVCC operations)
    pub fn get_latest_version(&self, record_id: u64) -> Option<Vec<u8>> {
        let chains = self.version_chains.lock().unwrap();
        chains.get(&record_id)
            .and_then(|chain| chain.get_latest_committed())
            .map(|version| version.data.clone())
    }

    /// Vacuum old versions (cleanup)
    pub fn vacuum(&self, before_timestamp: u64) -> Result<usize> {
        let mut chains = self.version_chains.lock().unwrap();
        let mut removed_count = 0;

        for chain in chains.values_mut() {
            let original_len = chain.versions.len();
            chain.versions.retain(|v| {
                // Keep if created after timestamp or still active
                v.visibility.commit_ts >= before_timestamp ||
                self.active_transactions.lock().unwrap().contains(&v.visibility.creator_id)
            });
            removed_count += original_len - chain.versions.len();
        }

        // Remove empty chains
        chains.retain(|_, chain| !chain.versions.is_empty());

        Ok(removed_count)
    }

    /// Get statistics
    pub fn get_stats(&self) -> MVCCStats {
        let chains = self.version_chains.lock().unwrap();
        let active = self.active_transactions.lock().unwrap();
        let timestamp = self.global_timestamp.lock().unwrap();

        let mut total_versions = 0;
        let mut total_chains = chains.len();

        for chain in chains.values() {
            total_versions += chain.versions.len();
        }

        MVCCStats {
            total_records: total_chains,
            total_versions,
            active_transactions: active.len(),
            current_timestamp: *timestamp,
        }
    }
}

/// MVCC statistics
#[derive(Debug, Clone)]
pub struct MVCCStats {
    /// Total number of records
    pub total_records: usize,
    /// Total number of versions across all records
    pub total_versions: usize,
    /// Number of active transactions
    pub active_transactions: usize,
    /// Current global timestamp
    pub current_timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visibility_info() {
        let mut visibility = VisibilityInfo::new(1, 100);

        assert!(visibility.is_visible(2, 150));
        assert!(!visibility.is_visible(1, 50)); // Can't see own before commit

        visibility.delete(2, 200);
        assert!(!visibility.is_visible(3, 250)); // Deleted before snapshot
        assert!(visibility.is_visible(3, 150)); // Still visible before delete
    }

    #[test]
    fn test_version_chain() {
        let mut chain = VersionChain::new();

        // Add version 1
        let version1 = RecordVersion::new(b"data1".to_vec(), 1, 100, 1);
        chain.add_version(version1);

        // Add version 2
        let version2 = RecordVersion::new(b"data2".to_vec(), 2, 200, 2);
        chain.add_version(version2);

        // Transaction 3 should see version 2
        let visible = chain.get_visible_version(3, 250);
        assert!(visible.is_some());
        assert_eq!(visible.unwrap().data, b"data2");

        // Transaction 1 at timestamp 50 should see nothing
        let visible = chain.get_visible_version(1, 50);
        assert!(visible.is_none());
    }

    #[test]
    fn test_mvcc_manager() -> Result<()> {
        let mvcc = MVCCManager::new();

        // Begin transaction
        let snapshot = mvcc.begin_transaction(1);

        // Insert record
        mvcc.insert_version(100, b"test data".to_vec(), 1, 1)?;

        // Read from same transaction (should not see own uncommitted)
        let data = mvcc.read_record(100, &snapshot)?;
        assert!(data.is_none());

        // Commit transaction
        mvcc.commit_transaction(1, snapshot.timestamp);

        // Begin new transaction
        let snapshot2 = mvcc.begin_transaction(2);

        // Should see committed data
        let data = mvcc.read_record(100, &snapshot2)?;
        assert!(data.is_some());
        assert_eq!(data.unwrap(), b"test data");

        Ok(())
    }

    #[test]
    fn test_concurrent_transactions() -> Result<()> {
        let mvcc = MVCCManager::new();

        // Begin two transactions
        let snapshot1 = mvcc.begin_transaction(1);
        let snapshot2 = mvcc.begin_transaction(2);

        // Insert record in transaction 1
        mvcc.insert_version(100, b"data1".to_vec(), 1, 1)?;

        // Both transactions shouldn't see uncommitted data
        assert!(mvcc.read_record(100, &snapshot1)?.is_none());
        assert!(mvcc.read_record(100, &snapshot2)?.is_none());

        // Commit transaction 1
        mvcc.commit_transaction(1, snapshot1.timestamp);

        // Transaction 2 still shouldn't see it (snapshot was taken before commit)
        assert!(mvcc.read_record(100, &snapshot2)?.is_none());

        // Commit transaction 2 and start new one
        mvcc.commit_transaction(2, snapshot2.timestamp);
        let snapshot3 = mvcc.begin_transaction(3);

        // New transaction should see committed data
        assert!(mvcc.read_record(100, &snapshot3)?.is_some());

        Ok(())
    }
}
