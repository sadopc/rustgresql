//! Transaction management module
//!
//! Provides ACID transaction support with MVCC and WAL

pub mod manager;
pub mod wal;
pub mod mvcc;
pub mod lock;
pub mod ddl_transaction;
pub mod ddl_wal;

pub use manager::{TransactionManager, Transaction, TransactionState};
pub use crate::TransactionId;
pub use wal::{WALManager, WALRecord, WALRecordType};
pub use mvcc::{MVCCManager, VersionChain, RecordVersion};
pub use lock::{LockManager, LockType, LockMode};
pub use ddl_transaction::*;
pub use ddl_wal::{DdlWALManager, DdlTransactionState, init_ddl_wal_manager, get_ddl_wal_manager};