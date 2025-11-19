//! File manager for handling disk I/O operations

use crate::error::Result;
use crate::{PageId, storage::{Page, PageType}};
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::Path;

/// Database file header containing metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseHeader {
    /// Magic number to identify database files
    pub magic_number: u64,
    /// Version of the database format
    pub version: u32,
    /// Page size in bytes
    pub page_size: u32,
    /// Total number of pages in the file
    pub total_pages: u64,
    /// Number of free pages available
    pub free_pages: u64,
    /// First free page in the free list
    pub first_free_page: Option<PageId>,
    /// Checksum of the header
    pub checksum: u32,
}

impl DatabaseHeader {
    /// Create a new database header
    pub fn new(page_size: u32) -> Self {
        Self {
            magic_number: 0x5253514C44544142, // "RUSTGDB" in hex
            version: 1,
            page_size,
            total_pages: 1, // Header page counts as page 0
            free_pages: 0,
            first_free_page: None,
            checksum: 0,
        }
    }

    /// Calculate and update the checksum
    pub fn update_checksum(&mut self) {
        use crc::Crc;

        // Zero out checksum for calculation
        let _old_checksum = self.checksum;
        self.checksum = 0;

        let hasher = Crc::<u32>::new(&crc::CRC_32_ISCSI);
        let mut checksum = hasher.digest();

        // Serialize header without checksum
        let bytes = bincode::serialize(&self).unwrap();
        checksum.update(&bytes[..bytes.len() - 4]); // Exclude checksum field

        self.checksum = checksum.finalize();
    }

    /// Verify the checksum
    pub fn verify(&self) -> bool {
        use crc::Crc;

        let hasher = Crc::<u32>::new(&crc::CRC_32_ISCSI);
        let mut checksum = hasher.digest();

        let bytes = bincode::serialize(&self).unwrap();
        checksum.update(&bytes[..bytes.len() - 4]);

        checksum.finalize() == self.checksum
    }
}

/// Trait for file management operations
pub trait FileManager: Send + Sync {
    /// Read a page from disk
    fn read_page(&self, page_id: PageId) -> Result<Page>;

    /// Write a page to disk
    fn write_page(&self, page_id: PageId, page: Page) -> Result<()>;

    /// Allocate a new page
    fn allocate_page(&self, page_type: PageType) -> Result<PageId>;

    /// Deallocate a page (add to free list)
    fn deallocate_page(&self, page_id: PageId) -> Result<()>;

    /// Flush all pending writes to disk
    fn sync(&self) -> Result<()>;

    /// Get database statistics
    fn get_stats(&self) -> Result<DatabaseStats>;
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub total_pages: u64,
    pub free_pages: u64,
    pub used_pages: u64,
}

/// Default file manager implementation
pub struct DefaultFileManager {
    file: std::sync::Mutex<File>,
    header: std::sync::Mutex<DatabaseHeader>,
    pub page_size: usize,
}

impl DefaultFileManager {
    /// Open an existing database file
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;

        let mut fm = Self {
            file: std::sync::Mutex::new(file),
            header: std::sync::Mutex::new(DatabaseHeader::new(8192)),
            page_size: 8192,
        };

        // Read and verify header
        fm.read_header()?;

        Ok(fm)
    }

    /// Create a new database file
    pub fn create<P: AsRef<Path>>(path: P, page_size: u32) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        let mut header = DatabaseHeader::new(page_size);
        header.update_checksum();

        let fm = Self {
            file: std::sync::Mutex::new(file),
            header: std::sync::Mutex::new(header),
            page_size: page_size as usize,
        };

        // Write header to file
        fm.write_header()?;

        Ok(fm)
    }

    /// Read the database header from file
    fn read_header(&mut self) -> Result<()> {
        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(0))?;

        let header_bytes = {
            let mut buf = vec![0u8; 1024]; // Max header size
            let bytes_read = file.read(&mut buf)?;
            buf.truncate(bytes_read);
            buf
        };

        if header_bytes.is_empty() {
            return Err(crate::error::RustgreSQLError::Storage(
                "Empty database file".to_string()
            ));
        }

        let header: DatabaseHeader = bincode::deserialize(&header_bytes)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        if !header.verify() {
            return Err(crate::error::RustgreSQLError::Corruption(
                "Invalid header checksum".to_string()
            ));
        }

        if header.magic_number != DatabaseHeader::new(0).magic_number {
            return Err(crate::error::RustgreSQLError::Corruption(
                "Invalid magic number".to_string()
            ));
        }

        *self.header.lock().unwrap() = header.clone();
        self.page_size = header.page_size as usize;

        Ok(())
    }

    /// Write the database header to file
    fn write_header(&self) -> Result<()> {
        let mut header = self.header.lock().unwrap();
        header.update_checksum();

        let header_bytes = bincode::serialize(&*header)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&header_bytes)?;
        file.flush()?;

        Ok(())
    }

    /// Get the file offset for a page
    fn page_offset(&self, page_id: PageId) -> Result<u64> {
        let header = self.header.lock().unwrap();
        if page_id == 0 {
            return Err(crate::error::RustgreSQLError::Storage(
                "Cannot access header page through read_page".to_string()
            ));
        }

        // Header page (0) is at offset 0, data pages start after header
        let header_size = bincode::serialize(&*header).unwrap().len();
        let header_pages = (header_size + self.page_size - 1) / self.page_size;

        Ok(((header_pages + (page_id - 1) as usize) * self.page_size) as u64)
    }
}

impl FileManager for DefaultFileManager {
    fn read_page(&self, page_id: PageId) -> Result<Page> {
        let offset = self.page_offset(page_id)?;
        let mut file = self.file.lock().unwrap();

        file.seek(SeekFrom::Start(offset))?;

        let mut page_bytes = vec![0u8; self.page_size];
        let bytes_read = file.read(&mut page_bytes)
            .map_err(|e| crate::error::RustgreSQLError::Storage(
                format!("Failed to read page {}: {}", page_id, e)
            ))?;

        // Check if we read the complete page
        if bytes_read != self.page_size {
            return Err(crate::error::RustgreSQLError::Storage(
                format!("Incomplete read for page {}: expected {} bytes, got {}",
                       page_id, self.page_size, bytes_read)
            ));
        }

        // Attempt to deserialize the page
        match Page::from_bytes(&page_bytes) {
            Ok(page) => {
                // Verify page integrity
                if page.verify() {
                    Ok(page)
                } else {
                    // If checksum fails, this indicates data corruption
                    // For now, return an error instead of silently continuing
                    Err(crate::error::RustgreSQLError::Corruption(
                        format!("Page {} failed checksum verification - data corruption detected", page_id)
                    ))
                }
            }
            Err(e) => {
                // If deserialization fails, this also indicates corruption
                Err(crate::error::RustgreSQLError::Corruption(
                    format!("Failed to deserialize page {}: {}", page_id, e)
                ))
            }
        }
    }

    fn write_page(&self, page_id: PageId, mut page: Page) -> Result<()> {
        let offset = self.page_offset(page_id)?;
        let mut file = self.file.lock().unwrap();

        // Update checksum before writing
        page.update_checksum();

        // Seek to the page offset
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| crate::error::RustgreSQLError::Storage(
                format!("Failed to seek to page {} offset: {}", page_id, e)
            ))?;

        // Serialize the page
        let page_bytes = page.to_bytes()
            .map_err(|e| crate::error::RustgreSQLError::Storage(
                format!("Failed to serialize page {}: {}", page_id, e)
            ))?;

        // Verify the serialized data size
        if page_bytes.len() != self.page_size {
            return Err(crate::error::RustgreSQLError::Storage(
                format!("Page {} serialization produced wrong size: expected {}, got {}",
                       page_id, self.page_size, page_bytes.len())
            ));
        }

        // Write the page data
        file.write_all(&page_bytes)
            .map_err(|e| crate::error::RustgreSQLError::Storage(
                format!("Failed to write page {}: {}", page_id, e)
            ))?;

        // Force write to disk for durability
        file.flush()
            .map_err(|e| crate::error::RustgreSQLError::Storage(
                format!("Failed to flush page {}: {}", page_id, e)
            ))?;

        file.sync_all()
            .map_err(|e| crate::error::RustgreSQLError::Storage(
                format!("Failed to sync page {}: {}", page_id, e)
            ))?;

        Ok(())
    }

    fn allocate_page(&self, page_type: PageType) -> Result<PageId> {
        let mut header = self.header.lock().unwrap();

        let allocated_page_id = if let Some(free_page_id) = header.first_free_page {
            // Use page from free list
            match self.read_page(free_page_id) {
                Ok(page) => {
                    // Update free list
                    header.first_free_page = page.header.next_page_id;
                    header.free_pages -= 1;

                    drop(header);

                    // Mark page as allocated
                    let mut allocated_page = page;
                    allocated_page.header.page_type = page_type;
                    allocated_page.header.next_page_id = None;

                    // Write the updated page
                    if let Err(e) = self.write_page(free_page_id, allocated_page) {
                        // If write fails, we need to rollback the header change
                        let mut header = self.header.lock().unwrap();
                        header.first_free_page = Some(free_page_id);
                        header.free_pages += 1;
                        return Err(e);
                    }

                    free_page_id
                }
                Err(e) => {
                    // If reading the free page fails, remove it from free list and try again
                    eprintln!("Warning: Failed to read free page {}, removing from free list: {}", free_page_id, e);
                    header.first_free_page = header.first_free_page.filter(|&id| id != free_page_id);
                    drop(header);
                    return self.allocate_page(page_type); // Recursive call to try again
                }
            }
        } else {
            // Allocate new page at end of file
            let new_page_id = header.total_pages;
            header.total_pages += 1;

            drop(header);

            // Create page with proper initialization order
            let mut new_page = Page::new(new_page_id, page_type);
            // Update checksum AFTER page is fully initialized
            new_page.update_checksum();

            // Write the new page
            self.write_page(new_page_id, new_page)
                .map_err(|e| {
                    // If write fails, rollback the header change
                    let mut header = self.header.lock().unwrap();
                    header.total_pages -= 1;
                    e
                })?;

            new_page_id
        };

        // Update header on disk
        self.write_header()?;

        Ok(allocated_page_id)
    }

    fn deallocate_page(&self, page_id: PageId) -> Result<()> {
        let mut header = self.header.lock().unwrap();

        // Validate page_id is within valid range
        if page_id >= header.total_pages {
            return Err(crate::error::RustgreSQLError::Storage(
                format!("Invalid page ID {} for deallocation (max: {})", page_id, header.total_pages - 1)
            ));
        }

        // Read the page to get its current state
        let mut page = match self.read_page(page_id) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Warning: Failed to read page {} for deallocation: {}", page_id, e);
                // Create a new free page as fallback
                Page::new(page_id, PageType::Free)
            }
        };

        // Add to free list
        page.header.page_type = PageType::Free;
        page.header.next_page_id = header.first_free_page;
        header.first_free_page = Some(page_id);
        header.free_pages += 1;

        drop(header);

        // Write the updated page
        self.write_page(page_id, page)
            .map_err(|e| {
                // If write fails, rollback the header change
                let mut header = self.header.lock().unwrap();
                header.first_free_page = header.first_free_page.filter(|&id| id != page_id);
                header.free_pages -= 1;
                e
            })?;

        // Update header on disk
        self.write_header()?;

        Ok(())
    }

    fn sync(&self) -> Result<()> {
        let mut file = self.file.lock().unwrap();
        file.flush()?;
        file.sync_all()?;
        Ok(())
    }

    fn get_stats(&self) -> Result<DatabaseStats> {
        let header = self.header.lock().unwrap();
        Ok(DatabaseStats {
            total_pages: header.total_pages,
            free_pages: header.free_pages,
            used_pages: header.total_pages - header.free_pages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_database_header() {
        let mut header = DatabaseHeader::new(8192);
        header.update_checksum();

        assert!(header.verify());

        // Corrupt header
        header.version = 999;
        assert!(!header.verify());
    }

    #[test]
    fn test_file_manager_create_and_open() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.db");

        // Create database
        let fm = DefaultFileManager::create(&file_path, 8192).unwrap();
        let stats = fm.get_stats().unwrap();
        assert_eq!(stats.total_pages, 1); // Only header page
        assert_eq!(stats.free_pages, 0);

        drop(fm);

        // Open existing database
        let fm = DefaultFileManager::open(&file_path).unwrap();
        let stats = fm.get_stats().unwrap();
        assert_eq!(stats.total_pages, 1);
        assert_eq!(stats.free_pages, 0);
    }

    #[test]
    fn test_page_allocation_and_deallocation() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.db");

        let fm = DefaultFileManager::create(&file_path, 8192).unwrap();

        // Allocate a page
        let page_id = fm.allocate_page(PageType::Data).unwrap();
        assert_eq!(page_id, 1);

        let stats = fm.get_stats().unwrap();
        assert_eq!(stats.total_pages, 2);
        assert_eq!(stats.free_pages, 0);

        // Write and read page
        let page = Page::new(page_id, PageType::Data);
        fm.write_page(page_id, page.clone()).unwrap();

        let read_page = fm.read_page(page_id).unwrap();
        assert_eq!(read_page.header.page_id, page_id);
        assert_eq!(read_page.header.page_type, PageType::Data);

        // Deallocate page
        fm.deallocate_page(page_id).unwrap();

        let stats = fm.get_stats().unwrap();
        assert_eq!(stats.total_pages, 2);
        assert_eq!(stats.free_pages, 1);

        // Allocate again (should reuse deallocated page)
        let new_page_id = fm.allocate_page(PageType::BTreeLeaf).unwrap();
        assert_eq!(new_page_id, page_id); // Should reuse the deallocated page
    }

    #[test]
    fn test_invalid_magic_number() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("invalid.db");

        // Create a file with invalid magic number
        {
            let mut file = File::create(&file_path).unwrap();
            file.write_all(b"INVALID_DATA").unwrap();
        }

        // Try to open as database
        let result = DefaultFileManager::open(&file_path);
        assert!(result.is_err());
    }
}