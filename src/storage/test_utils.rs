//! Test utilities for storage module

use crate::{error::Result, PageId, storage::{Page, PageType, file_manager::FileManager}};
use std::collections::HashMap;
use std::sync::Mutex;

/// Mock file manager for testing
pub struct MockFileManager {
    pub pages: Mutex<HashMap<PageId, Page>>,
    pub next_page_id: Mutex<PageId>,
}

impl MockFileManager {
    pub fn new() -> Self {
        Self {
            pages: Mutex::new(HashMap::new()),
            next_page_id: Mutex::new(1),
        }
    }
}

impl FileManager for MockFileManager {
    fn read_page(&self, page_id: PageId) -> Result<Page> {
        let pages = self.pages.lock().unwrap();
        pages.get(&page_id)
            .cloned()
            .ok_or_else(|| crate::error::RustgreSQLError::PageNotFound(page_id))
    }

    fn write_page(&self, page_id: PageId, page: Page) -> Result<()> {
        let mut pages = self.pages.lock().unwrap();
        pages.insert(page_id, page);
        Ok(())
    }

    fn allocate_page(&self, page_type: PageType) -> Result<PageId> {
        let mut next_id = self.next_page_id.lock().unwrap();
        let page_id = *next_id;
        *next_id += 1;

        let page = Page::new(page_id, page_type);
        self.pages.lock().unwrap().insert(page_id, page);

        Ok(page_id)
    }

    fn deallocate_page(&self, page_id: PageId) -> Result<()> {
        self.pages.lock().unwrap().remove(&page_id);
        Ok(())
    }

    fn sync(&self) -> Result<()> {
        Ok(())
    }

    fn get_stats(&self) -> Result<crate::storage::file_manager::DatabaseStats> {
        let pages = self.pages.lock().unwrap();
        Ok(crate::storage::file_manager::DatabaseStats {
            total_pages: pages.len() as u64,
            free_pages: 0,
            used_pages: pages.len() as u64,
        })
    }
}