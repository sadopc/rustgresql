//! Transaction manager
//!
//! Coordinates WAL, MVCC, and lock management for ACID transactions

use crate::{error::RustgreSQLError, Result, TransactionId};
use crate::transaction::{wal::{WALManager, WALRecord}, mvcc::{MVCCManager, Snapshot}};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Transaction isolation levels
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum IsolationLevel {
    /// Read Uncommitted - lowest isolation, may read uncommitted data
    ReadUncommitted,
    /// Read Committed - cannot read uncommitted data
    ReadCommitted,
    /// Repeatable Read - same query returns same rows within transaction
    RepeatableRead,
    /// Serializable - full isolation, transactions appear to run serially
    Serializable,
}

/// Transaction state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransactionState {
    Active,
    Preparing,
    Committed,
    Aborted,
}

/// Transaction context
#[derive(Debug, Clone)]
pub struct Transaction {
    /// Transaction ID
    pub id: TransactionId,
    /// Current state
    pub state: TransactionState,
    /// Snapshot for MVCC
    pub snapshot: Option<Snapshot>,
    /// Isolation level
    pub isolation_level: IsolationLevel,
    /// Start timestamp
    pub start_ts: u64,
    /// Last LSN used by this transaction
    pub last_lsn: Option<crate::transaction::wal::LSN>,
    /// Modified pages (for rollback)
    pub modified_pages: HashMap<crate::PageId, Vec<u8>>,
}

impl Transaction {
    /// Create new transaction
    pub fn new(id: TransactionId, isolation_level: IsolationLevel, start_ts: u64) -> Self {
        Self {
            id,
            state: TransactionState::Active,
            snapshot: None,
            isolation_level,
            start_ts,
            last_lsn: None,
            modified_pages: HashMap::new(),
        }
    }

    /// Check if transaction is active
    pub fn is_active(&self) -> bool {
        self.state == TransactionState::Active
    }

    /// Add modified page for rollback
    pub fn add_modified_page(&mut self, page_id: crate::PageId, old_data: Vec<u8>) {
        self.modified_pages.insert(page_id, old_data);
    }

    /// Get modified page data
    pub fn get_modified_page(&self, page_id: crate::PageId) -> Option<&Vec<u8>> {
        self.modified_pages.get(&page_id)
    }
}

/// Transaction manager
#[derive(Debug)]
pub struct TransactionManager {
    /// Next transaction ID
    next_transaction_id: Arc<Mutex<TransactionId>>,
    /// Active transactions
    active_transactions: Arc<Mutex<HashMap<TransactionId, Transaction>>>,
    /// WAL manager
    wal: Option<Arc<Mutex<WALManager>>>,
    /// MVCC manager
    mvcc: Arc<Mutex<MVCCManager>>,
}

impl TransactionManager {
    /// Create new transaction manager
    pub fn new() -> Self {
        Self {
            next_transaction_id: Arc::new(Mutex::new(1)),
            active_transactions: Arc::new(Mutex::new(HashMap::new())),
            wal: None,
            mvcc: Arc::new(Mutex::new(MVCCManager::new())),
        }
    }

    /// Create transaction manager with WAL
    pub fn with_wal(wal: Arc<Mutex<WALManager>>) -> Self {
        Self {
            next_transaction_id: Arc::new(Mutex::new(1)),
            active_transactions: Arc::new(Mutex::new(HashMap::new())),
            wal: Some(wal),
            mvcc: Arc::new(Mutex::new(MVCCManager::new())),
        }
    }

    /// Begin a new transaction
    pub fn begin_transaction(&self, isolation_level: IsolationLevel) -> Result<TransactionId> {
        // Generate transaction ID
        let transaction_id = {
            let mut next_id = self.next_transaction_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        // Get start timestamp
        let start_ts = {
            let mvcc = self.mvcc.lock().unwrap();
            let snapshot = mvcc.begin_transaction(transaction_id);
            snapshot.timestamp
        };

        // Create transaction object
        let transaction = Transaction::new(transaction_id, isolation_level, start_ts);

        // Get snapshot for MVCC
        let snapshot = {
            let mvcc = self.mvcc.lock().unwrap();
            mvcc.begin_transaction(transaction_id)
        };

        let mut transaction = transaction;
        transaction.snapshot = Some(snapshot);

        // Register transaction
        {
            let mut active = self.active_transactions.lock().unwrap();
            active.insert(transaction_id, transaction);
        }

        // Write WAL record if available
        if let Some(ref wal) = self.wal {
            let wal_guard = wal.lock().unwrap();
            let record = WALRecord::begin(transaction_id, 0);
            wal_guard.append_record(record)?;
        }

        Ok(transaction_id)
    }

    /// Commit a transaction
    pub fn commit_transaction(&self, transaction_id: TransactionId) -> Result<()> {
        // Get transaction
        let snapshot = {
            let mut active = self.active_transactions.lock().unwrap();
            let transaction = active.remove(&transaction_id)
                .ok_or_else(|| RustgreSQLError::Transaction(
                    format!("Transaction {} not found", transaction_id)
                ))?;
            let snapshot = transaction.snapshot.as_ref().unwrap().clone();
            // Put transaction back
            active.insert(transaction_id, transaction);
            snapshot
        };

        // Get transaction again for modification
        let mut transaction = {
            let mut active = self.active_transactions.lock().unwrap();
            active.remove(&transaction_id).unwrap()
        };

        // Two-phase commit
        // Phase 1: Prepare
        transaction.state = TransactionState::Preparing;
        let commit_lsn = if let Some(ref wal) = self.wal {
            let mut wal_guard = wal.lock().unwrap();
            let record = WALRecord::commit(
                transaction_id,
                0,
                transaction.last_lsn,
            );
            let lsn = wal_guard.append_record(record)?;
            wal_guard.flush_buffer()?;
            Some(lsn)
        } else {
            None
        };

        // Phase 2: Commit
        transaction.state = TransactionState::Committed;

        // Update MVCC
        {
            let mut mvcc = self.mvcc.lock().unwrap();
            mvcc.commit_transaction(transaction_id, snapshot.timestamp);
        }

        // Write final commit record
        if let (Some(ref wal), Some(lsn)) = (&self.wal, commit_lsn) {
            let mut wal_guard = wal.lock().unwrap();
            let record = WALRecord::commit(
                transaction_id,
                lsn,
                Some(lsn - 1),
            );
            wal_guard.append_record(record)?;
            wal_guard.flush_buffer()?;
        }

        log::debug!("Transaction {} committed", transaction_id);
        Ok(())
    }

    /// Rollback a transaction
    pub fn rollback_transaction(&self, transaction_id: TransactionId) -> Result<()> {
        // Get transaction
        let transaction = {
            let mut active = self.active_transactions.lock().unwrap();
            active.remove(&transaction_id)
                .ok_or_else(|| RustgreSQLError::Transaction(
                    format!("Transaction {} not found", transaction_id)
                ))?
        };

        let snapshot = transaction.snapshot.as_ref().unwrap().clone();

        // Write abort record to WAL
        if let Some(ref wal) = self.wal {
            let mut wal_guard = wal.lock().unwrap();
            let record = WALRecord::abort(
                transaction_id,
                0,
                transaction.last_lsn,
            );
            wal_guard.append_record(record)?;
            wal_guard.flush_buffer()?;
        }

        // Rollback modified pages
        for (page_id, old_data) in &transaction.modified_pages {
            log::debug!("Rolling back page {} to original state", page_id);
            // In a real implementation, this would restore the page from old_data
            // For now, we just log it
        }

        // Update MVCC (abort removes all uncommitted versions)
        {
            let mut mvcc = self.mvcc.lock().unwrap();
            mvcc.abort_transaction(transaction_id);
        }

        log::debug!("Transaction {} rolled back", transaction_id);
        Ok(())
    }

    /// Get transaction by ID
    pub fn get_transaction(&self, transaction_id: TransactionId) -> Option<Transaction> {
        let active = self.active_transactions.lock().unwrap();
        active.get(&transaction_id).cloned()
    }

    /// Check if transaction is active
    pub fn is_transaction_active(&self, transaction_id: TransactionId) -> bool {
        let active = self.active_transactions.lock().unwrap();
        active.contains_key(&transaction_id)
    }

    /// Log a modification in WAL
    pub fn log_modify(
        &self,
        transaction_id: TransactionId,
        page_id: crate::PageId,
        offset: usize,
        old_data: Option<Vec<u8>>,
        new_data: Vec<u8>,
    ) -> Result<()> {
        // Update transaction's modified pages
        {
            let mut active = self.active_transactions.lock().unwrap();
            if let Some(transaction) = active.get_mut(&transaction_id) {
                if let Some(ref old) = old_data {
                    transaction.add_modified_page(page_id, old.clone());
                }
            }
        }

        // Write WAL record
        if let Some(ref wal) = self.wal {
            let mut wal_guard = wal.lock().unwrap();

            let record = match old_data {
                Some(old) => WALRecord::update(
                    transaction_id,
                    0,
                    None, // prev_lsn will be set
                    page_id,
                    offset,
                    old,
                    new_data,
                ),
                None => WALRecord::insert(
                    transaction_id,
                    0,
                    None, // prev_lsn will be set
                    page_id,
                    offset,
                    new_data,
                ),
            };

            let lsn = wal_guard.append_record(record)?;

            // Update transaction's last LSN
            {
                let mut active = self.active_transactions.lock().unwrap();
                if let Some(transaction) = active.get_mut(&transaction_id) {
                    transaction.last_lsn = Some(lsn);
                }
            }
        }

        Ok(())
    }

    /// Log a deletion in WAL
    pub fn log_delete(
        &self,
        transaction_id: TransactionId,
        page_id: crate::PageId,
        offset: usize,
        old_data: Vec<u8>,
    ) -> Result<()> {
        // Update transaction's modified pages
        {
            let mut active = self.active_transactions.lock().unwrap();
            if let Some(transaction) = active.get_mut(&transaction_id) {
                transaction.add_modified_page(page_id, old_data.clone());
            }
        }

        // Write WAL record
        if let Some(ref wal) = self.wal {
            let mut wal_guard = wal.lock().unwrap();
            let record = WALRecord::delete(
                transaction_id,
                0,
                None, // prev_lsn will be set
                page_id,
                offset,
                old_data,
            );

            let lsn = wal_guard.append_record(record)?;

            // Update transaction's last LSN
            {
                let mut active = self.active_transactions.lock().unwrap();
                if let Some(transaction) = active.get_mut(&transaction_id) {
                    transaction.last_lsn = Some(lsn);
                }
            }
        }

        Ok(())
    }

    /// Get MVCC manager reference
    pub fn mvcc(&self) -> Arc<Mutex<MVCCManager>> {
        self.mvcc.clone()
    }

    /// Get transaction statistics
    pub fn get_stats(&self) -> TransactionStats {
        let active = self.active_transactions.lock().unwrap();
        let mvcc = self.mvcc.lock().unwrap();
        let mvcc_stats = mvcc.get_stats();

        TransactionStats {
            active_transactions: active.len(),
            next_transaction_id: *self.next_transaction_id.lock().unwrap(),
            mvcc_stats,
        }
    }

    /// Cleanup old transactions and versions
    pub fn cleanup(&self) -> Result<CleanupResult> {
        let mvcc = self.mvcc.lock().unwrap();

        // Get oldest active transaction timestamp
        let oldest_snapshot = {
            let active = self.active_transactions.lock().unwrap();
            let mvcc_stats = mvcc.get_stats();
            active.values()
                .map(|t| t.snapshot.as_ref().unwrap().timestamp)
                .min()
                .unwrap_or(mvcc_stats.current_timestamp)
        };

        // Vacuum old versions
        let removed_versions = mvcc.vacuum(oldest_snapshot)?;

        Ok(CleanupResult {
            removed_versions,
            vacuum_timestamp: oldest_snapshot,
        })
    }

    /// Force checkpoint
    pub fn checkpoint(&self) -> Result<()> {
        if let Some(ref wal) = self.wal {
            let wal_guard = wal.lock().unwrap();
            // Get current LSN
            let lsn = wal_guard.get_next_lsn();
            wal_guard.checkpoint(lsn)?;
        }
        Ok(())
    }
}

/// Transaction statistics
#[derive(Debug, Clone)]
pub struct TransactionStats {
    /// Number of active transactions
    pub active_transactions: usize,
    /// Next transaction ID to be assigned
    pub next_transaction_id: TransactionId,
    /// MVCC statistics
    pub mvcc_stats: crate::transaction::mvcc::MVCCStats,
}

/// Cleanup result
#[derive(Debug, Clone)]
pub struct CleanupResult {
    /// Number of versions removed
    pub removed_versions: usize,
    /// Vacuum timestamp
    pub vacuum_timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_transaction_lifecycle() -> Result<()> {
        let tm = TransactionManager::new();

        // Begin transaction
        let tx_id = tm.begin_transaction(IsolationLevel::ReadCommitted)?;
        assert!(tm.is_transaction_active(tx_id));

        // Get transaction
        let tx = tm.get_transaction(tx_id).unwrap();
        assert_eq!(tx.id, tx_id);
        assert_eq!(tx.state, TransactionState::Active);
        assert_eq!(tx.isolation_level, IsolationLevel::ReadCommitted);

        // Commit transaction
        tm.commit_transaction(tx_id)?;
        assert!(!tm.is_transaction_active(tx_id));

        Ok(())
    }

    #[test]
    fn test_transaction_rollback() -> Result<()> {
        let tm = TransactionManager::new();

        // Begin transaction
        let tx_id = tm.begin_transaction(IsolationLevel::ReadCommitted)?;
        assert!(tm.is_transaction_active(tx_id));

        // Log some modification
        tm.log_modify(tx_id, 1, 0, None, b"new data".to_vec())?;

        // Rollback transaction
        tm.rollback_transaction(tx_id)?;
        assert!(!tm.is_transaction_active(tx_id));

        Ok(())
    }

    #[test]
    fn test_multiple_transactions() -> Result<()> {
        let tm = TransactionManager::new();

        // Begin multiple transactions
        let tx1 = tm.begin_transaction(IsolationLevel::ReadCommitted)?;
        let tx2 = tm.begin_transaction(IsolationLevel::Serializable)?;

        assert!(tm.is_transaction_active(tx1));
        assert!(tm.is_transaction_active(tx2));

        // Verify they have different IDs
        assert_ne!(tx1, tx2);

        // Commit one
        tm.commit_transaction(tx1)?;
        assert!(!tm.is_transaction_active(tx1));
        assert!(tm.is_transaction_active(tx2));

        // Commit the other
        tm.commit_transaction(tx2)?;
        assert!(!tm.is_transaction_active(tx1));
        assert!(!tm.is_transaction_active(tx2));

        Ok(())
    }

    #[test]
    fn test_transaction_stats() -> Result<()> {
        let tm = TransactionManager::new();

        // Initial stats
        let stats = tm.get_stats();
        assert_eq!(stats.active_transactions, 0);

        // Begin transaction
        let tx_id = tm.begin_transaction(IsolationLevel::ReadCommitted)?;

        // Stats should show active transaction
        let stats = tm.get_stats();
        assert_eq!(stats.active_transactions, 1);
        assert_eq!(stats.next_transaction_id, 2);

        // Commit
        tm.commit_transaction(tx_id)?;

        // Stats should show no active transactions
        let stats = tm.get_stats();
        assert_eq!(stats.active_transactions, 0);

        Ok(())
    }
}