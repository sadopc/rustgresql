//! Page management for the storage engine
//!
//! Defines the page structure and basic operations

use crate::{error::Result, PageId};
use serde::{Deserialize, Serialize};

/// Default page size (8KB)
pub const DEFAULT_PAGE_SIZE: usize = 8192;

/// Page types
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PageType {
    /// Header page containing database metadata
    Header,
    /// B-Tree internal node
    BTreeInternal,
    /// B-Tree leaf node
    BTreeLeaf,
    /// Data page for table records
    Data,
    /// Write-Ahead Log page
    WAL,
    /// Free page available for reuse
    Free,
}

/// Page header structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageHeader {
    /// Page ID
    pub page_id: PageId,
    /// Page type
    pub page_type: PageType,
    /// Previous page in linked list (for some page types)
    pub prev_page_id: Option<PageId>,
    /// Next page in linked list (for some page types)
    pub next_page_id: Option<PageId>,
    /// Number of free bytes in page
    pub free_bytes: usize,
    /// Checksum for page integrity
    pub checksum: u32,
}

/// Page structure containing header and data
#[derive(Debug, Clone)]
pub struct Page {
    /// Page header
    pub header: PageHeader,
    /// Raw page data
    pub data: Vec<u8>,
}

impl Page {
    /// Create a new page with the specified ID and type
    pub fn new(page_id: PageId, page_type: PageType) -> Self {
        let data = vec![0u8; DEFAULT_PAGE_SIZE];

        Self {
            header: PageHeader {
                page_id,
                page_type,
                prev_page_id: None,
                next_page_id: None,
                free_bytes: DEFAULT_PAGE_SIZE, // All data space is initially free
                checksum: 0,
            },
            data,
        }
    }

    /// Calculate page checksum
    pub fn calculate_checksum(&self) -> u32 {
        use crc::Crc;

        let hasher = Crc::<u32>::new(&crc::CRC_32_ISCSI);
        let mut digest = hasher.digest();

        // Create a temporary header with checksum set to 0
        let mut temp_header = self.header.clone();
        temp_header.checksum = 0;

        // Serialize the temporary header
        let header_bytes = bincode::serialize(&temp_header).unwrap();

        // Build the full page bytes exactly as in to_bytes
        let mut page_bytes = header_bytes;
        page_bytes.extend_from_slice(&self.data);
        page_bytes.resize(DEFAULT_PAGE_SIZE, 0);

        // Update digest with the full page bytes
        digest.update(&page_bytes);

        digest.finalize()
    }

    /// Verify page integrity using checksum
    pub fn verify(&self) -> bool {
        self.calculate_checksum() == self.header.checksum
    }

    /// Update checksum after modifications
    pub fn update_checksum(&mut self) {
        self.header.checksum = self.calculate_checksum();
    }

    /// Get available free space in page
    pub fn free_space(&self) -> usize {
        self.header.free_bytes
    }

    /// Reserve space in page for data
    pub fn reserve_space(&mut self, size: usize) -> Result<usize> {
        if size > self.header.free_bytes {
            return Err(crate::error::RustgreSQLError::Storage(
                format!("Not enough space in page: requested {}, available {}",
                       size, self.header.free_bytes)
            ));
        }

        let offset = DEFAULT_PAGE_SIZE - self.header.free_bytes;
        self.header.free_bytes -= size;
        Ok(offset)
    }

    /// Serialize page to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut page_bytes = Vec::with_capacity(DEFAULT_PAGE_SIZE);

        // Serialize header
        let header_bytes = bincode::serialize(&self.header)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        page_bytes.extend_from_slice(&header_bytes);

        // Add data
        page_bytes.extend_from_slice(&self.data);

        // Ensure page is exactly DEFAULT_PAGE_SIZE
        page_bytes.resize(DEFAULT_PAGE_SIZE, 0);

        Ok(page_bytes)
    }

    /// Deserialize page from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != DEFAULT_PAGE_SIZE {
            return Err(crate::error::RustgreSQLError::Storage(
                format!("Invalid page size: expected {}, got {}",
                        DEFAULT_PAGE_SIZE, bytes.len())
            ));
        }

        // Determine header size by serializing a dummy header
        let dummy_header = PageHeader {
            page_id: 0,
            page_type: PageType::Data,
            prev_page_id: None,
            next_page_id: None,
            free_bytes: 0,
            checksum: 0,
        };
        let header_size = bincode::serialize(&dummy_header)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?.len();

        let header_bytes = &bytes[..header_size];
        let header: PageHeader = bincode::deserialize(header_bytes)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        let data = bytes[header_size..].to_vec();

        Ok(Self { header, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_creation() {
        let page = Page::new(1, PageType::Data);
        assert_eq!(page.header.page_id, 1);
        assert_eq!(page.header.page_type, PageType::Data);
        assert_eq!(page.data.len(), DEFAULT_PAGE_SIZE);
    }

    #[test]
    fn test_page_checksum() {
        let mut page = Page::new(1, PageType::Data);
        page.update_checksum();

        assert!(page.verify());

        // Corrupt data
        page.data[0] = 1;
        assert!(!page.verify());
    }

    #[test]
    fn test_page_serialization() {
        let page = Page::new(1, PageType::Data);
        let bytes = page.to_bytes().unwrap();
        assert_eq!(bytes.len(), DEFAULT_PAGE_SIZE);

        let deserialized = Page::from_bytes(&bytes).unwrap();
        assert_eq!(deserialized.header.page_id, page.header.page_id);
        assert_eq!(deserialized.header.page_type, page.header.page_type);
    }

    #[test]
    fn test_reserve_space() {
        let mut page = Page::new(1, PageType::Data);
        let initial_free = page.free_space();

        let offset = page.reserve_space(100).unwrap();
        assert_eq!(offset, DEFAULT_PAGE_SIZE - initial_free);
        assert_eq!(page.free_space(), initial_free - 100);

        // Try to reserve more than available
        assert!(page.reserve_space(page.free_space() + 1).is_err());
    }
}