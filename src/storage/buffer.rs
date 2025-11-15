//! Buffer pool management for efficient page caching

use crate::error::Result;
use crate::PageId;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};

/// Buffer pool frame containing a page and metadata
#[derive(Debug)]
struct BufferFrame {
    /// The page data
    page: Option<crate::storage::Page>,
    /// Page ID if page is loaded
    page_id: Option<PageId>,
    /// Reference count for the page
    pin_count: u32,
    /// Whether the page is dirty (needs to be written to disk)
    dirty: bool,
    /// LRU list position
    lru_index: Option<usize>,
}

impl BufferFrame {
    fn new() -> Self {
        Self {
            page: None,
            page_id: None,
            pin_count: 0,
            dirty: false,
            lru_index: None,
        }
    }

    /// Pin the page (increment reference count)
    fn pin(&mut self) {
        self.pin_count += 1;
    }

    /// Unpin the page (decrement reference count)
    fn unpin(&mut self) -> Result<()> {
        if self.pin_count == 0 {
            return Err(crate::error::RustgreSQLError::Internal(
                "Attempt to unpin page with pin_count = 0".to_string()
            ));
        }
        self.pin_count -= 1;
        Ok(())
    }

    /// Check if page is available for eviction
    fn is_evictable(&self) -> bool {
        self.pin_count == 0
    }
}

/// Buffer pool for managing in-memory pages
#[derive(Debug)]
pub struct BufferPool {
    /// Buffer frames
    frames: Vec<Mutex<BufferFrame>>,
    /// LRU list for eviction policy
    lru_list: Arc<Mutex<VecDeque<PageId>>>,
    /// Page ID to frame index mapping
    page_to_frame: Arc<RwLock<HashMap<PageId, usize>>>,
    /// Frame index to Page ID mapping
    frame_to_page: Arc<RwLock<HashMap<usize, PageId>>>,
}

impl BufferPool {
    /// Create a new buffer pool with specified capacity
    pub fn new(capacity: usize) -> Self {
        let mut frames = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            frames.push(Mutex::new(BufferFrame::new()));
        }

        Self {
            frames,
            lru_list: Arc::new(Mutex::new(VecDeque::new())),
            page_to_frame: Arc::new(RwLock::new(HashMap::new())),
            frame_to_page: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the capacity of the buffer pool
    pub fn capacity(&self) -> usize {
        self.frames.len()
    }

    /// Get the number of pages currently in the buffer
    pub fn size(&self) -> usize {
        self.page_to_frame.read().unwrap().len()
    }

    /// Find an available frame for loading a page
    fn find_available_frame(&self) -> Result<usize> {
        // First, try to find an empty frame
        for (i, frame) in self.frames.iter().enumerate() {
            let f = frame.lock().unwrap();
            if f.page_id.is_none() {
                return Ok(i);
            }
        }

        // If no empty frames, try to evict using LRU
        {
            let mut lru_list = self.lru_list.lock().unwrap();
            let page_to_frame = self.page_to_frame.read().unwrap();
            let _frame_to_page = self.frame_to_page.read().unwrap();

            while let Some(&page_id) = lru_list.front() {
                if let Some(&frame_idx) = page_to_frame.get(&page_id) {
                    let frame = &self.frames[frame_idx];
                    let f = frame.lock().unwrap();
                    if f.is_evictable() {
                        lru_list.pop_front();
                        return Ok(frame_idx);
                    }
                }
                lru_list.pop_front();
            }
        }

        Err(crate::error::RustgreSQLError::Storage(
            "No available buffer frames".to_string()
        ))
    }

    /// Update LRU list for a page
    fn update_lru(&self, page_id: PageId) {
        let mut lru_list = self.lru_list.lock().unwrap();

        // Remove page from current position if it exists
        lru_list.retain(|&pid| pid != page_id);
        // Add to the back (most recently used)
        lru_list.push_back(page_id);
    }
}

/// Buffer pool manager that coordinates between buffer pool and file manager
pub struct BufferPoolManager {
    pub buffer_pool: BufferPool,
    pub file_manager: std::sync::Arc<Mutex<dyn crate::storage::FileManager + Send>>,
}

impl std::fmt::Debug for BufferPoolManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferPoolManager")
            .field("buffer_pool", &self.buffer_pool)
            .field("file_manager", &"<FileManager>")
            .finish()
    }
}

impl BufferPoolManager {
    /// Create a new buffer pool manager
    pub fn new(
        capacity: usize,
        file_manager: std::sync::Arc<Mutex<dyn crate::storage::FileManager + Send>>,
    ) -> Self {
        Self {
            buffer_pool: BufferPool::new(capacity),
            file_manager,
        }
    }

    /// Fetch a page from buffer pool or disk
    pub fn fetch_page(&self, page_id: PageId) -> Result<Arc<Mutex<crate::storage::Page>>> {
        // Check if page is already in buffer pool
        {
            let page_to_frame = self.buffer_pool.page_to_frame.read().unwrap();
            if let Some(&frame_idx) = page_to_frame.get(&page_id) {
                let frame = &self.buffer_pool.frames[frame_idx];
                let mut f = frame.lock().unwrap();
                f.pin();
                self.buffer_pool.update_lru(page_id);

                if let Some(ref page) = f.page {
                    return Ok(Arc::new(Mutex::new(page.clone())));
                }
            }
        }

        // Page not in buffer, load from disk
        let frame_idx = self.buffer_pool.find_available_frame()?;
        let frame = &self.buffer_pool.frames[frame_idx];

        {
            let mut f = frame.lock().unwrap();

            // If frame contains a dirty page, write it back
            if f.dirty {
                if let (Some(old_page_id), Some(ref old_page)) = (f.page_id, f.page.clone()) {
                    let fm = self.file_manager.lock().unwrap();
                    fm.write_page(old_page_id, old_page.clone())?;
                }
            }

            // Update mappings
            {
                let mut page_to_frame = self.buffer_pool.page_to_frame.write().unwrap();
                let mut frame_to_page = self.buffer_pool.frame_to_page.write().unwrap();

                // Remove old mapping if exists
                if let Some(old_page_id) = f.page_id {
                    page_to_frame.remove(&old_page_id);
                    frame_to_page.remove(&frame_idx);
                }

                // Add new mapping
                page_to_frame.insert(page_id, frame_idx);
                frame_to_page.insert(frame_idx, page_id);
            }

            // Load page from disk
            let fm = self.file_manager.lock().unwrap();
            let page = fm.read_page(page_id)?;

            f.page = Some(page.clone());
            f.page_id = Some(page_id);
            f.pin();
            f.dirty = false;

            self.buffer_pool.update_lru(page_id);

            Ok(Arc::new(Mutex::new(page)))
        }
    }

    /// Unpin a page
    pub fn unpin_page(&self, page_id: PageId, dirty: bool) -> Result<()> {
        let page_to_frame = self.buffer_pool.page_to_frame.read().unwrap();
        if let Some(&frame_idx) = page_to_frame.get(&page_id) {
            let frame = &self.buffer_pool.frames[frame_idx];
            let mut f = frame.lock().unwrap();
            f.unpin()?;
            f.dirty = f.dirty || dirty;
            Ok(())
        } else {
            Err(crate::error::RustgreSQLError::PageNotFound(page_id))
        }
    }

    /// Flush all dirty pages to disk
    pub fn flush_all_pages(&self) -> Result<()> {
        let page_to_frame = self.buffer_pool.page_to_frame.read().unwrap();
        let frame_to_page = self.buffer_pool.frame_to_page.read().unwrap();

        for (&frame_idx, &page_id) in frame_to_page.iter() {
            let frame = &self.buffer_pool.frames[frame_idx];
            let f = frame.lock().unwrap();

            if f.dirty && f.page.is_some() {
                let fm = self.file_manager.lock().unwrap();
                fm.write_page(page_id, f.page.as_ref().unwrap().clone())?;
            }
        }

        Ok(())
    }

    /// Create a new page in the database
    pub fn new_page(&self, page_type: crate::storage::PageType) -> Result<PageId> {
        let fm = self.file_manager.lock().unwrap();
        let page_id = fm.allocate_page(page_type)?;
        drop(fm);

        // Pin the newly created page
        self.fetch_page(page_id)?;

        Ok(page_id)
    }

    /// Delete a page from the database
    pub fn delete_page(&self, page_id: PageId) -> Result<()> {
        // Remove from buffer pool
        {
            let mut page_to_frame = self.buffer_pool.page_to_frame.write().unwrap();
            let mut frame_to_page = self.buffer_pool.frame_to_page.write().unwrap();

            if let Some(&frame_idx) = page_to_frame.get(&page_id) {
                page_to_frame.remove(&page_id);
                frame_to_page.remove(&frame_idx);

                let frame = &self.buffer_pool.frames[frame_idx];
                let mut f = frame.lock().unwrap();
                f.page = None;
                f.page_id = None;
                f.dirty = false;
                f.pin_count = 0;
            }
        }

        // Remove from file manager
        {
            let fm = self.file_manager.lock().unwrap();
            fm.deallocate_page(page_id)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{Page, PageType};

    // MockFileManager moved to test_utils.rs module

    #[test]
    fn test_buffer_pool_creation() {
        let pool = BufferPool::new(10);
        assert_eq!(pool.capacity(), 10);
        assert_eq!(pool.size(), 0);
    }

    #[test]
    fn test_buffer_frame_pinning() {
        let mut frame = BufferFrame::new();
        assert_eq!(frame.pin_count, 0);
        assert!(frame.is_evictable());

        frame.pin();
        assert_eq!(frame.pin_count, 1);
        assert!(!frame.is_evictable());

        frame.unpin().unwrap();
        assert_eq!(frame.pin_count, 0);
        assert!(frame.is_evictable());
    }

    #[test]
    fn test_buffer_pool_manager_fetch_and_pin() {
        let file_manager = std::sync::Arc::new(Mutex::new(
            crate::storage::test_utils::MockFileManager::new()
        ));
        let bpm = BufferPoolManager::new(5, file_manager);

        // Create a new page
        let page_id = bpm.new_page(PageType::Data).unwrap();

        // Fetch the page
        let page = bpm.fetch_page(page_id).unwrap();
        assert_eq!(page.lock().unwrap().header.page_id, page_id);

        // Try to fetch again (should be cached)
        let page2 = bpm.fetch_page(page_id).unwrap();
        assert_eq!(page2.lock().unwrap().header.page_id, page_id);

        // Unpin
        bpm.unpin_page(page_id, false).unwrap();
    }
}