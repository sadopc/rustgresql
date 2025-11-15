//! Write-Ahead Logging
//!
//! Provides durability for transactions by logging changes before they are applied

use crate::{Result, PageId};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// WAL record types
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WALRecordType {
    /// Transaction begin
    Begin,
    /// Transaction commit
    Commit,
    /// Transaction abort/rollback
    Abort,
    /// Insert record
    Insert,
    /// Update record
    Update,
    /// Delete record
    Delete,
    /// Checkpoint marker
    Checkpoint,

    // DDL Operations
    /// CREATE TABLE
    CreateTable,
    /// DROP TABLE
    DropTable,
    /// CREATE INDEX
    CreateIndex,
    /// DROP INDEX
    DropIndex,
    /// ALTER TABLE ADD COLUMN
    AlterTableAddColumn,
    /// ALTER TABLE DROP COLUMN
    AlterTableDropColumn,
    /// ALTER TABLE ADD CONSTRAINT
    AlterTableAddConstraint,
    /// ALTER TABLE DROP CONSTRAINT
    AlterTableDropConstraint,

    // Schema Evolution
    /// Schema version change
    SchemaVersionChange,
    /// Table metadata update
    TableMetadataUpdate,
    /// Column metadata update
    ColumnMetadataUpdate,
    /// Index metadata update
    IndexMetadataUpdate,
}

/// WAL log sequence number (LSN)
pub type LSN = u64;

/// WAL record header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WALRecordHeader {
    /// Log sequence number
    pub lsn: LSN,
    /// Previous LSN in the same transaction
    pub prev_lsn: Option<LSN>,
    /// Transaction ID
    pub transaction_id: crate::TransactionId,
    /// Record type
    pub record_type: WALRecordType,
    /// Size of record data in bytes
    pub data_size: u32,
    /// Checksum for this record
    pub checksum: u32,
}

/// Insert/Update record data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifyRecord {
    /// Page ID being modified
    pub page_id: PageId,
    /// Offset within page
    pub offset: usize,
    /// Old data (for undo)
    pub old_data: Option<Vec<u8>>,
    /// New data (for redo)
    pub new_data: Vec<u8>,
}

/// DDL operation record data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdlRecord {
    /// Type of DDL operation
    pub ddl_type: String,
    /// Object name (table, index, etc.)
    pub object_name: String,
    /// Object type (table, index, column, constraint)
    pub object_type: String,
    /// Schema name
    pub schema_name: Option<String>,
    /// Serialized object definition (for create operations)
    pub object_definition: Option<Vec<u8>>,
    /// Previous object definition (for drop/alter operations)
    pub previous_definition: Option<Vec<u8>>,
    /// Additional metadata (column names, constraint types, etc.)
    pub metadata: std::collections::HashMap<String, String>,
    /// Dependent objects that will be affected
    pub dependents: Vec<String>,
}

/// Complete WAL record data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WALRecordData {
    /// Modify record (insert/update/delete)
    Modify(ModifyRecord),
    /// DDL record
    Ddl(DdlRecord),
}

/// Complete WAL record
#[derive(Debug, Clone)]
pub struct WALRecord {
    /// Record header
    pub header: WALRecordHeader,
    /// Optional record data
    pub data: Option<WALRecordData>,
}

impl WALRecord {
    /// Create a new transaction begin record
    pub fn begin(transaction_id: crate::TransactionId, lsn: LSN) -> Self {
        Self {
            header: WALRecordHeader {
                lsn,
                prev_lsn: None,
                transaction_id,
                record_type: WALRecordType::Begin,
                data_size: 0,
                checksum: 0,
            },
            data: None,
        }
    }

    /// Create a new transaction commit record
    pub fn commit(transaction_id: crate::TransactionId, lsn: LSN, prev_lsn: Option<LSN>) -> Self {
        Self {
            header: WALRecordHeader {
                lsn,
                prev_lsn,
                transaction_id,
                record_type: WALRecordType::Commit,
                data_size: 0,
                checksum: 0,
            },
            data: None,
        }
    }

    /// Create a new transaction abort record
    pub fn abort(transaction_id: crate::TransactionId, lsn: LSN, prev_lsn: Option<LSN>) -> Self {
        Self {
            header: WALRecordHeader {
                lsn,
                prev_lsn,
                transaction_id,
                record_type: WALRecordType::Abort,
                data_size: 0,
                checksum: 0,
            },
            data: None,
        }
    }

    /// Create a new insert record
    pub fn insert(
        transaction_id: crate::TransactionId,
        lsn: LSN,
        prev_lsn: Option<LSN>,
        page_id: PageId,
        offset: usize,
        new_data: Vec<u8>,
    ) -> Self {
        let data = ModifyRecord {
            page_id,
            offset,
            old_data: None,
            new_data,
        };
        let data_size = bincode::serialize(&data).unwrap().len() as u32;

        Self {
            header: WALRecordHeader {
                lsn,
                prev_lsn,
                transaction_id,
                record_type: WALRecordType::Insert,
                data_size,
                checksum: 0,
            },
            data: Some(WALRecordData::Modify(data)),
        }
    }

    /// Create a new update record
    pub fn update(
        transaction_id: crate::TransactionId,
        lsn: LSN,
        prev_lsn: Option<LSN>,
        page_id: PageId,
        offset: usize,
        old_data: Vec<u8>,
        new_data: Vec<u8>,
    ) -> Self {
        let data = ModifyRecord {
            page_id,
            offset,
            old_data: Some(old_data),
            new_data,
        };
        let data_size = bincode::serialize(&data).unwrap().len() as u32;

        Self {
            header: WALRecordHeader {
                lsn,
                prev_lsn,
                transaction_id,
                record_type: WALRecordType::Update,
                data_size,
                checksum: 0,
            },
            data: Some(WALRecordData::Modify(data)),
        }
    }

    /// Create a new delete record
    pub fn delete(
        transaction_id: crate::TransactionId,
        lsn: LSN,
        prev_lsn: Option<LSN>,
        page_id: PageId,
        offset: usize,
        old_data: Vec<u8>,
    ) -> Self {
        let data = ModifyRecord {
            page_id,
            offset,
            old_data: Some(old_data),
            new_data: vec![],
        };
        let data_size = bincode::serialize(&data).unwrap().len() as u32;

        Self {
            header: WALRecordHeader {
                lsn,
                prev_lsn,
                transaction_id,
                record_type: WALRecordType::Delete,
                data_size,
                checksum: 0,
            },
            data: Some(WALRecordData::Modify(data)),
        }
    }

    /// Create a new DDL record
    pub fn ddl(
        transaction_id: crate::TransactionId,
        lsn: LSN,
        prev_lsn: Option<LSN>,
        record_type: WALRecordType,
        ddl_type: String,
        object_name: String,
        object_type: String,
        schema_name: Option<String>,
        object_definition: Option<Vec<u8>>,
        previous_definition: Option<Vec<u8>>,
        metadata: std::collections::HashMap<String, String>,
        dependents: Vec<String>,
    ) -> Self {
        let data = DdlRecord {
            ddl_type,
            object_name,
            object_type,
            schema_name,
            object_definition,
            previous_definition,
            metadata,
            dependents,
        };
        let data_size = bincode::serialize(&data).unwrap().len() as u32;

        Self {
            header: WALRecordHeader {
                lsn,
                prev_lsn,
                transaction_id,
                record_type,
                data_size,
                checksum: 0,
            },
            data: Some(WALRecordData::Ddl(data)),
        }
    }

    /// Create a CREATE TABLE record
    pub fn create_table(
        transaction_id: crate::TransactionId,
        lsn: LSN,
        prev_lsn: Option<LSN>,
        table_name: &str,
        schema_name: Option<String>,
        table_definition: Vec<u8>,
        dependents: Vec<String>,
    ) -> Self {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("operation".to_string(), "CREATE".to_string());

        Self::ddl(
            transaction_id,
            lsn,
            prev_lsn,
            WALRecordType::CreateTable,
            "CREATE_TABLE".to_string(),
            table_name.to_string(),
            "table".to_string(),
            schema_name,
            Some(table_definition),
            None,
            metadata,
            dependents,
        )
    }

    /// Create a DROP TABLE record
    pub fn drop_table(
        transaction_id: crate::TransactionId,
        lsn: LSN,
        prev_lsn: Option<LSN>,
        table_name: &str,
        schema_name: Option<String>,
        previous_definition: Vec<u8>,
        dependents: Vec<String>,
    ) -> Self {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("operation".to_string(), "DROP".to_string());

        Self::ddl(
            transaction_id,
            lsn,
            prev_lsn,
            WALRecordType::DropTable,
            "DROP_TABLE".to_string(),
            table_name.to_string(),
            "table".to_string(),
            schema_name,
            None,
            Some(previous_definition),
            metadata,
            dependents,
        )
    }

    /// Create an ALTER TABLE record
    pub fn alter_table(
        transaction_id: crate::TransactionId,
        lsn: LSN,
        prev_lsn: Option<LSN>,
        record_type: WALRecordType,
        table_name: &str,
        schema_name: Option<String>,
        alteration_type: &str,
        target_name: &str,
        previous_definition: Option<Vec<u8>>,
        new_definition: Option<Vec<u8>>,
        dependents: Vec<String>,
    ) -> Self {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("alteration_type".to_string(), alteration_type.to_string());
        metadata.insert("target_name".to_string(), target_name.to_string());

        Self::ddl(
            transaction_id,
            lsn,
            prev_lsn,
            record_type,
            format!("ALTER_TABLE_{}", alteration_type),
            table_name.to_string(),
            "table".to_string(),
            schema_name,
            new_definition,
            previous_definition,
            metadata,
            dependents,
        )
    }

    /// Calculate and update checksum
    pub fn update_checksum(&mut self) {
        use crc::Crc;

        // Store original checksum
        let original_checksum = self.header.checksum;
        self.header.checksum = 0;

        // Serialize header and data
        let header_bytes = bincode::serialize(&self.header).unwrap();
        let mut all_bytes = header_bytes;

        if let Some(ref data) = self.data {
            let data_bytes = bincode::serialize(data).unwrap();
            all_bytes.extend_from_slice(&data_bytes);
        }

        // Calculate checksum
        let hasher = Crc::<u32>::new(&crc::CRC_32_ISCSI);
        self.header.checksum = hasher.digest().finalize();

        // Restore original if we're not updating
        if original_checksum != 0 {
            self.header.checksum = original_checksum;
        }
    }

    /// Verify checksum
    pub fn verify(&self) -> bool {
        let mut copy = self.clone();
        let original_checksum = copy.header.checksum;
        copy.header.checksum = 0;

        let header_bytes = bincode::serialize(&copy.header).unwrap();
        let mut all_bytes = header_bytes;

        if let Some(ref data) = copy.data {
            let data_bytes = bincode::serialize(data).unwrap();
            all_bytes.extend_from_slice(&data_bytes);
        }

        use crc::Crc;
        let hasher = Crc::<u32>::new(&crc::CRC_32_ISCSI);
        hasher.digest().finalize() == original_checksum
    }

    /// Serialize record to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut record = self.clone();
        record.update_checksum();

        let header_bytes = bincode::serialize(&record.header)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        let mut bytes = header_bytes;

        if let Some(ref data) = record.data {
            let data_bytes = bincode::serialize(data)
                .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;
            bytes.extend_from_slice(&data_bytes);
        }

        Ok(bytes)
    }

    /// Deserialize record from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            return Err(crate::error::RustgreSQLError::Serialization(
                "Empty WAL record".to_string()
            ));
        }

        let header_size = bincode::serialized_size(&WALRecordHeader {
            lsn: 0,
            prev_lsn: None,
            transaction_id: 0,
            record_type: WALRecordType::Begin,
            data_size: 0,
            checksum: 0,
        }).unwrap() as usize;

        if bytes.len() < header_size {
            return Err(crate::error::RustgreSQLError::Serialization(
                "Insufficient bytes for WAL header".to_string()
            ));
        }

        let header_bytes = &bytes[..header_size];
        let header: WALRecordHeader = bincode::deserialize(header_bytes)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        let data = if header.data_size > 0 {
            let data_bytes = &bytes[header_size..];

            // Determine data type based on record type
            let record_data = match header.record_type {
                WALRecordType::Insert | WALRecordType::Update | WALRecordType::Delete => {
                    let modify_record: ModifyRecord = bincode::deserialize(data_bytes)
                        .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;
                    WALRecordData::Modify(modify_record)
                }
                WALRecordType::CreateTable | WALRecordType::DropTable | WALRecordType::CreateIndex |
                WALRecordType::DropIndex | WALRecordType::AlterTableAddColumn | WALRecordType::AlterTableDropColumn |
                WALRecordType::AlterTableAddConstraint | WALRecordType::AlterTableDropConstraint |
                WALRecordType::SchemaVersionChange | WALRecordType::TableMetadataUpdate |
                WALRecordType::ColumnMetadataUpdate | WALRecordType::IndexMetadataUpdate => {
                    let ddl_record: DdlRecord = bincode::deserialize(data_bytes)
                        .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;
                    WALRecordData::Ddl(ddl_record)
                }
                _ => {
                    return Err(crate::error::RustgreSQLError::Serialization(
                        format!("Unexpected record type for data: {:?}", header.record_type)
                    ));
                }
            };

            Some(record_data)
        } else {
            None
        };

        Ok(Self { header, data })
    }
}

/// WAL file header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WALHeader {
    /// Magic number to identify WAL files
    pub magic_number: u64,
    /// Version of the WAL format
    pub version: u32,
    /// Page size
    pub page_size: u32,
    /// Starting LSN
    pub start_lsn: LSN,
}

impl WALHeader {
    /// Create new WAL header
    pub fn new(page_size: u32) -> Self {
        Self {
            magic_number: 0x57414C5253514C44, // "WALRSQLD" in hex
            version: 1,
            page_size,
            start_lsn: 0,
        }
    }
}

/// WAL manager for handling log records
#[derive(Debug)]
pub struct WALManager {
    /// WAL file
    file: Arc<Mutex<File>>,
    /// Next LSN to assign
    next_lsn: Mutex<LSN>,
    /// Buffered records for batch writing
    buffer: Mutex<Vec<WALRecord>>,
    /// Buffer size threshold
    buffer_size: usize,
    /// Whether fsync is enabled
    fsync_enabled: bool,
}

impl WALManager {
    /// Create new WAL manager
    pub fn create<P: AsRef<Path>>(path: P, page_size: u32) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        let manager = Self {
            file: Arc::new(Mutex::new(file)),
            next_lsn: Mutex::new(1), // Start from 1, 0 is reserved
            buffer: Mutex::new(Vec::new()),
            buffer_size: 1000,
            fsync_enabled: true,
        };

        // Write WAL header
        let header = WALHeader::new(page_size);
        let header_bytes = bincode::serialize(&header)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        {
            let mut file = manager.file.lock().unwrap();
            file.write_all(&header_bytes)?;
            file.flush()?;
        }

        Ok(manager)
    }

    /// Open existing WAL manager
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;

        let file_arc = Arc::new(Mutex::new(file));

        // Read and verify header
        let header_size = {
            let mut file_guard = file_arc.lock().unwrap();
            file_guard.seek(SeekFrom::Start(0))?;

            let header_size = bincode::serialized_size(&WALHeader::new(0)).unwrap() as usize;
            let mut header_bytes = vec![0u8; header_size];
            file_guard.read_exact(&mut header_bytes)?;

            let header: WALHeader = bincode::deserialize(&header_bytes)
                .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

            if header.magic_number != WALHeader::new(0).magic_number {
                return Err(crate::error::RustgreSQLError::Corruption(
                    "Invalid WAL magic number".to_string()
                ));
            }

            header_size
        };

        // Find next LSN by scanning records
        let mut next_lsn = 0u64;

        {
            let mut file_guard = file_arc.lock().unwrap();
            file_guard.seek(SeekFrom::Start(header_size as u64))?;

            loop {
                // Read header size bytes first
                let record_header_size = bincode::serialized_size(&WALRecordHeader {
                    lsn: 0,
                    prev_lsn: None,
                    transaction_id: 0,
                    record_type: WALRecordType::Begin,
                    data_size: 0,
                    checksum: 0,
                }).unwrap() as usize;

                let mut header_buf = vec![0u8; record_header_size];
                let bytes_read = file_guard.read(&mut header_buf)?;
                if bytes_read == 0 {
                    break; // End of file
                }
                if bytes_read < record_header_size {
                    return Err(crate::error::RustgreSQLError::Corruption(
                        "Incomplete WAL record header".to_string()
                    ));
                }

                let record_header: WALRecordHeader = bincode::deserialize(&header_buf)
                    .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

                // Skip data if present
                if record_header.data_size > 0 {
                    file_guard.seek(SeekFrom::Current(record_header.data_size as i64))?;
                }

                next_lsn = record_header.lsn + 1;
            }
        }

        Ok(Self {
            file: file_arc,
            next_lsn: Mutex::new(next_lsn),
            buffer: Mutex::new(Vec::new()),
            buffer_size: 1000,
            fsync_enabled: true,
        })
    }

    /// Append a record to the WAL
    pub fn append_record(&self, mut record: WALRecord) -> Result<LSN> {
        // Assign LSN
        {
            let mut next_lsn = self.next_lsn.lock().unwrap();
            record.header.lsn = *next_lsn;
            *next_lsn += 1;
        }

        // Buffer the record
        {
            let mut buffer = self.buffer.lock().unwrap();
            buffer.push(record.clone());

            // Flush if buffer is full
            if buffer.len() >= self.buffer_size {
                drop(buffer);
                self.flush_buffer()?;
            }
        }

        Ok(record.header.lsn)
    }

    /// Force flush of all buffered records
    pub fn flush_buffer(&self) -> Result<()> {
        let mut buffer = self.buffer.lock().unwrap();
        if buffer.is_empty() {
            return Ok(());
        }

        let records: Vec<_> = buffer.drain(..).collect();
        drop(buffer);

        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::End(0))?;

        for record in records {
            let bytes = record.to_bytes()?;
            file.write_all(&bytes)?;
        }

        file.flush()?;

        if self.fsync_enabled {
            file.sync_all()?;
        }

        Ok(())
    }

    /// Set buffer size
    pub fn set_buffer_size(&mut self, size: usize) {
        self.buffer_size = size;
    }

    /// Enable/disable fsync
    pub fn set_fsync(&mut self, enabled: bool) {
        self.fsync_enabled = enabled;
    }

    /// Get next LSN
    pub fn get_next_lsn(&self) -> LSN {
        *self.next_lsn.lock().unwrap()
    }

    /// Checkpoint: mark records as no longer needed for recovery
    pub fn checkpoint(&self, lsn: LSN) -> Result<()> {
        // For now, just ensure all records are flushed
        self.flush_buffer()?;

        // In a full implementation, this would:
        // 1. Write a checkpoint record
        // 2. Allow truncation of log before checkpoint LSN
        // 3. Update the WAL header

        let record = WALRecord {
            header: WALRecordHeader {
                lsn: self.get_next_lsn(),
                prev_lsn: None,
                transaction_id: 0, // System transaction
                record_type: WALRecordType::Checkpoint,
                data_size: 8, // Size of LSN
                checksum: 0,
            },
            data: None, // Checkpoint LSN would be stored here
        };

        self.append_record(record)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_wal_record_creation() {
        let record = WALRecord::begin(1, 100);
        assert_eq!(record.header.transaction_id, 1);
        assert_eq!(record.header.lsn, 100);
        assert_eq!(record.header.record_type, WALRecordType::Begin);
    }

    #[test]
    fn test_wal_record_serialization() {
        let record = WALRecord::insert(1, 100, None, 42, 10, b"test data".to_vec());
        let bytes = record.to_bytes().unwrap();
        let deserialized = WALRecord::from_bytes(&bytes).unwrap();

        assert_eq!(deserialized.header.transaction_id, 1);
        assert_eq!(deserialized.header.lsn, 100);
        assert_eq!(deserialized.header.record_type, WALRecordType::Insert);
    }

    #[test]
    fn test_wal_record_checksum() {
        let mut record = WALRecord::update(1, 100, None, 42, 10, b"old".to_vec(), b"new".to_vec());

        record.update_checksum();
        assert!(record.verify());

        // Corrupt data
        if let Some(ref mut record_data) = record.data {
            if let crate::transaction::wal::WALRecordData::Modify(modify_record) = record_data {
                modify_record.new_data[0] = 255;
            }
        }
        assert!(!record.verify());
    }

    #[test]
    fn test_wal_manager() -> Result<()> {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test.wal");

        let wal = WALManager::create(&wal_path, 8192)?;

        // Append some records
        let begin_lsn = wal.append_record(WALRecord::begin(1, 0))?;
        assert_eq!(begin_lsn, 1);

        let insert_lsn = wal.append_record(WALRecord::insert(1, 0, Some(begin_lsn), 1, 0, b"data".to_vec()))?;
        assert_eq!(insert_lsn, 2);

        let commit_lsn = wal.append_record(WALRecord::commit(1, 0, Some(insert_lsn)))?;
        assert_eq!(commit_lsn, 3);

        wal.flush_buffer()?;

        // Reopen WAL
        drop(wal);
        let wal_reopened = WALManager::open(&wal_path)?;

        // Next LSN should be 4
        assert_eq!(wal_reopened.get_next_lsn(), 4);

        Ok(())
    }
}
