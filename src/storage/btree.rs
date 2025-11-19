//! B-Tree implementation for indexed storage

use crate::error::Result;
use crate::PageId;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::VecDeque;

/// Maximum number of keys in a B-Tree node
const MAX_KEYS: usize = 100;
/// Minimum number of keys in a B-Tree node (except root)
const MIN_KEYS: usize = MAX_KEYS / 2;

/// B-Tree key type
pub type BTreeKey = Vec<u8>;

/// B-Tree value type (page ID for data or child node)
pub type BTreeValue = PageId;

/// B-Tree node entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BTreeEntry {
    pub key: BTreeKey,
    pub value: BTreeValue,
}

/// B-Tree node types
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BTreeNodeType {
    /// Leaf node containing data entries
    Leaf,
    /// Internal node containing keys and child pointers
    Internal,
}

/// B-Tree node structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BTreeNode {
    /// Node type (leaf or internal)
    pub node_type: BTreeNodeType,
    /// Number of entries in the node
    pub entry_count: usize,
    /// Entries (key-value pairs)
    pub entries: Vec<BTreeEntry>,
    /// Child page IDs for internal nodes
    pub children: Vec<PageId>,
    /// Previous node in linked list (for leaf nodes)
    pub prev_leaf: Option<PageId>,
    /// Next node in linked list (for leaf nodes)
    pub next_leaf: Option<PageId>,
}

impl BTreeNode {
    /// Create a new empty B-Tree node
    pub fn new(node_type: BTreeNodeType) -> Self {
        Self {
            node_type,
            entry_count: 0,
            entries: Vec::with_capacity(MAX_KEYS),
            children: Vec::with_capacity(MAX_KEYS + 1),
            prev_leaf: None,
            next_leaf: None,
        }
    }

    /// Check if node is full
    pub fn is_full(&self) -> bool {
        self.entry_count >= MAX_KEYS
    }

    /// Check if node is empty
    pub fn is_empty(&self) -> bool {
        self.entry_count == 0
    }

    /// Check if node needs redistribution/merge
    pub fn is_underflow(&self) -> bool {
        self.entry_count < MIN_KEYS
    }

    /// Find the index where a key should be inserted
    pub fn find_insert_position(&self, key: &BTreeKey) -> usize {
        self.entries
            .binary_search_by(|entry| entry.key.as_slice().cmp(key.as_slice()))
            .unwrap_or_else(|pos| pos)
    }

    /// Find an exact key in the node
    pub fn find_key(&self, key: &BTreeKey) -> Option<usize> {
        self.entries
            .binary_search_by(|entry| entry.key.as_slice().cmp(key.as_slice()))
            .ok()
    }

    /// Insert an entry into the node
    pub fn insert_entry(&mut self, entry: BTreeEntry) -> Result<()> {
        if self.is_full() {
            return Err(crate::error::RustgreSQLError::Index(
                "Node is full".to_string()
            ));
        }

        let pos = self.find_insert_position(&entry.key);
        self.entries.insert(pos, entry);
        self.entry_count += 1;

        Ok(())
    }

    /// Remove an entry from the node
    pub fn remove_entry(&mut self, key: &BTreeKey) -> Result<BTreeEntry> {
        let pos = self.find_key(key)
            .ok_or_else(|| crate::error::RustgreSQLError::Index(
                "Key not found".to_string()
            ))?;

        let entry = self.entries.remove(pos);
        self.entry_count -= 1;

        Ok(entry)
    }

    /// Get the minimum key in the node
    pub fn min_key(&self) -> Option<&BTreeKey> {
        self.entries.first().map(|e| &e.key)
    }

    /// Get the maximum key in the node
    pub fn max_key(&self) -> Option<&BTreeKey> {
        self.entries.last().map(|e| &e.key)
    }

    /// Split the node into two nodes
    pub fn split(&mut self) -> (BTreeNode, BTreeEntry) {
        let split_pos = self.entry_count / 2;
        let mut right_node = BTreeNode::new(self.node_type);

        // Move entries to right node
        right_node.entries = self.entries.split_off(split_pos);
        right_node.entry_count = right_node.entries.len();

        // For internal nodes, also move children
        if self.node_type == BTreeNodeType::Internal {
            let child_split_pos = split_pos + 1;
            right_node.children = self.children.split_off(child_split_pos);
        }

        // Get the middle entry (separator)
        let separator = self.entries.pop().unwrap();
        self.entry_count -= 1;

        (right_node, separator)
    }
}

/// B-Tree implementation
#[derive(Debug)]
pub struct BTree {
    root_page_id: PageId,
    buffer_manager: std::sync::Arc<crate::storage::BufferPoolManager>,
}

impl BTree {
    /// Get the root page ID
    pub fn root_page_id(&self) -> PageId {
        self.root_page_id
    }

    /// Create a new B-Tree
    pub fn new(buffer_manager: std::sync::Arc<crate::storage::BufferPoolManager>) -> Result<Self> {
        // Create root node
        let root_page_id = buffer_manager.new_page(crate::storage::PageType::BTreeInternal)?;
        let root_page = buffer_manager.fetch_page(root_page_id)?;
        let mut root = root_page.lock().unwrap();

        // Initialize root as leaf node initially
        let node = BTreeNode::new(BTreeNodeType::Leaf);
        let node_bytes = bincode::serialize(&node)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        root.data[..node_bytes.len()].copy_from_slice(&node_bytes);
        root.header.free_bytes = root.data.len() - node_bytes.len();

        buffer_manager.unpin_page(root_page_id, true)?;

        Ok(Self {
            root_page_id,
            buffer_manager,
        })
    }

    /// Load an existing B-Tree from root page
    pub fn load(
        root_page_id: PageId,
        buffer_manager: std::sync::Arc<crate::storage::BufferPoolManager>,
    ) -> Self {
        Self {
            root_page_id,
            buffer_manager,
        }
    }

    /// Load a B-Tree node from disk
    fn load_node(&self, page_id: PageId) -> Result<(PageId, BTreeNode)> {
        let page = self.buffer_manager.fetch_page(page_id)?;
        let page_guard = page.lock().unwrap();

        // Calculate the actual size of serialized data
        let node_size = page_guard.data.len() - page_guard.header.free_bytes;

        // If node_size is 0 or invalid, this might be an uninitialized page
        if node_size == 0 || node_size > page_guard.data.len() {
            // Return a new empty leaf node for uninitialized pages
            let node = BTreeNode::new(BTreeNodeType::Leaf);
            return Ok((page_id, node));
        }

        let node_bytes = &page_guard.data[..node_size];

        // Additional validation: check if the data looks like valid serialized data
        if node_bytes.is_empty() || node_bytes.iter().all(|&b| b == 0) {
            // Return a new empty leaf node for pages with no valid data
            let node = BTreeNode::new(BTreeNodeType::Leaf);
            return Ok((page_id, node));
        }

        let node: BTreeNode = bincode::deserialize(node_bytes)
            .map_err(|e| {
                // If deserialization fails, log the error and return a new node
                eprintln!("Failed to deserialize B-Tree node from page {}: {}. Creating new node.", page_id, e);
                crate::error::RustgreSQLError::Serialization(e.to_string())
            })?;

        Ok((page_id, node))
    }

    /// Save a B-Tree node to disk
    fn save_node(&self, page_id: PageId, node: &BTreeNode) -> Result<()> {
        let page = self.buffer_manager.fetch_page(page_id)?;
        let mut page_guard = page.lock().unwrap();

        let node_bytes = bincode::serialize(node)
            .map_err(|e| crate::error::RustgreSQLError::Serialization(e.to_string()))?;

        // Ensure we don't exceed the page data capacity
        if node_bytes.len() > page_guard.data.len() {
            return Err(crate::error::RustgreSQLError::Storage(
                format!("B-Tree node too large for page: {} bytes, max {} bytes",
                       node_bytes.len(), page_guard.data.len())
            ));
        }

        // Clear the data area first
        page_guard.data.fill(0);

        // Write the serialized node data
        page_guard.data[..node_bytes.len()].copy_from_slice(&node_bytes);

        // Update free bytes (remaining space in data area)
        page_guard.header.free_bytes = page_guard.data.len() - node_bytes.len();

        // Update checksum
        page_guard.update_checksum();

        drop(page_guard);
        self.buffer_manager.unpin_page(page_id, true)?;

        Ok(())
    }

    /// Insert a key-value pair into the B-Tree
    pub fn insert(&mut self, key: BTreeKey, value: BTreeValue) -> Result<()> {
        let (mut root_id, mut root_node) = self.load_node(self.root_page_id)?;

        // If root is full, split it
        if root_node.is_full() {
            let new_root_id = self.buffer_manager.new_page(crate::storage::PageType::BTreeInternal)?;
            let mut new_root_node = BTreeNode::new(BTreeNodeType::Internal);

            let (right_node, separator) = root_node.split();
            self.save_node(root_id, &root_node)?;

            let right_page_id = self.buffer_manager.new_page(crate::storage::PageType::BTreeLeaf)?;
            self.save_node(right_page_id, &right_node)?;

            // Update new root
            new_root_node.children.push(root_id);
            new_root_node.children.push(right_page_id);
            new_root_node.entries.push(separator);
            new_root_node.entry_count = 1;

            self.save_node(new_root_id, &new_root_node)?;

            // Update root
            self.root_page_id = new_root_id;
            root_id = new_root_id;
            root_node = new_root_node;
        }

        // Insert into appropriate leaf
        self.insert_into_node(root_id, root_node, key, value)?;

        Ok(())
    }

    /// Insert a key-value pair into a node (recursive)
    fn insert_into_node(
        &mut self,
        node_id: PageId,
        mut node: BTreeNode,
        key: BTreeKey,
        value: BTreeValue,
    ) -> Result<()> {
        match node.node_type {
            BTreeNodeType::Leaf => {
                // Insert into leaf node
                node.insert_entry(BTreeEntry { key, value })?;
                self.save_node(node_id, &node)?;
                Ok(())
            }
            BTreeNodeType::Internal => {
                // Find appropriate child
                let child_idx = node.find_insert_position(&key);
                let child_id = node.children[child_idx];

                let (child_id, mut child_node) = self.load_node(child_id)?;

                // If child is full, split it
                if child_node.is_full() {
                    let (right_node, separator) = child_node.split();
                    self.save_node(child_id, &child_node)?;

                    let right_page_id = self.buffer_manager.new_page(
                        if right_node.node_type == BTreeNodeType::Leaf {
                            crate::storage::PageType::BTreeLeaf
                        } else {
                            crate::storage::PageType::BTreeInternal
                        }
                    )?;
                    self.save_node(right_page_id, &right_node)?;

                    // Insert separator into current node
                    node.entries.insert(child_idx, separator.clone());
                    node.children.insert(child_idx + 1, right_page_id);
                    node.entry_count += 1;

                    // Determine which child to insert into
                    let cmp_result = key.cmp(&separator.key);
                    let target_child_id = if cmp_result == Ordering::Less {
                        child_id
                    } else {
                        right_page_id
                    };

                    self.save_node(node_id, &node)?;

                    // Recurse into appropriate child
                    if target_child_id == child_id {
                        self.insert_into_node(child_id, child_node, key, value)?
                    } else {
                        self.insert_into_node(right_page_id, right_node, key, value)?
                    }
                } else {
                    self.save_node(node_id, &node)?;
                    self.insert_into_node(child_id, child_node, key, value)?;
                }

                Ok(())
            }
        }
    }

    /// Search for a key in the B-Tree
    pub fn search(&self, key: &BTreeKey) -> Result<Option<BTreeValue>> {
        let (_, node) = self.load_node(self.root_page_id)?;
        self.search_in_node(node, key)
    }

    /// Search for a key in a specific node (recursive)
    fn search_in_node(&self, node: BTreeNode, key: &BTreeKey) -> Result<Option<BTreeValue>> {
        match node.node_type {
            BTreeNodeType::Leaf => {
                // Search in leaf node
                Ok(node.find_key(key).map(|idx| node.entries[idx].value))
            }
            BTreeNodeType::Internal => {
                // Find appropriate child
                let child_idx = node.find_insert_position(key);
                let child_id = node.children[child_idx];

                let (_, child_node) = self.load_node(child_id)?;
                self.search_in_node(child_node, key)
            }
        }
    }

    /// Delete a key from the B-Tree
    pub fn delete(&mut self, key: &BTreeKey) -> Result<Option<BTreeValue>> {
        let (_, mut root_node) = self.load_node(self.root_page_id)?;
        let (deleted_value, should_merge_root) = self.delete_from_node(self.root_page_id, &mut root_node, key)?;

        // If root became empty and has children, make first child the new root
        if should_merge_root && !root_node.children.is_empty() {
            let new_root_id = root_node.children[0];
            self.buffer_manager.delete_page(self.root_page_id)?;
            self.root_page_id = new_root_id;
        }

        Ok(deleted_value)
    }

    /// Delete a key from a node (recursive)
    fn delete_from_node(
        &mut self,
        node_id: PageId,
        node: &mut BTreeNode,
        key: &BTreeKey,
    ) -> Result<(Option<BTreeValue>, bool)> {
        match node.node_type {
            BTreeNodeType::Leaf => {
                // Delete from leaf node
                match node.find_key(key) {
                    Some(idx) => {
                        let entry = node.entries.remove(idx);
                        node.entry_count -= 1;
                        self.save_node(node_id, node)?;
                        Ok((Some(entry.value), node.is_empty()))
                    }
                    None => Ok((None, false)),
                }
            }
            BTreeNodeType::Internal => {
                let child_idx = node.find_insert_position(key);
                let child_id = node.children[child_idx];

                let (_, mut child_node) = self.load_node(child_id)?;
                let (deleted_value, child_needs_merge) = self.delete_from_node(child_id, &mut child_node, key)?;

                if child_needs_merge {
                    // Try to borrow from siblings or merge
                    let should_merge_root = self.handle_underflow(node_id, node, child_idx)?;
                    self.save_node(node_id, node)?;
                    Ok((deleted_value, should_merge_root))
                } else {
                    self.save_node(child_id, &child_node)?;
                    Ok((deleted_value, false))
                }
            }
        }
    }

    /// Handle underflow in a child node
    fn handle_underflow(
        &mut self,
        node_id: PageId,
        node: &mut BTreeNode,
        child_idx: usize,
    ) -> Result<bool> {
        // Try to borrow from left sibling
        if child_idx > 0 {
            let left_child_id = node.children[child_idx - 1];
            let (_, left_child) = self.load_node(left_child_id)?;

            if left_child.entry_count > MIN_KEYS {
                // Borrow from left sibling
                return self.borrow_from_left(node, child_idx);
            }
        }

        // Try to borrow from right sibling
        if child_idx < node.children.len() - 1 {
            let right_child_id = node.children[child_idx + 1];
            let (_, right_child) = self.load_node(right_child_id)?;

            if right_child.entry_count > MIN_KEYS {
                // Borrow from right sibling
                return self.borrow_from_right(node, child_idx);
            }
        }

        // Merge with a sibling
        if child_idx > 0 {
            self.merge_with_left(node, child_idx)
        } else {
            self.merge_with_right(node, child_idx)
        }
    }

    /// Borrow from left sibling
    fn borrow_from_left(&mut self, _node: &mut BTreeNode, _child_idx: usize) -> Result<bool> {
        // Implementation for borrowing from left sibling
        Ok(false)
    }

    /// Borrow from right sibling
    fn borrow_from_right(&mut self, _node: &mut BTreeNode, _child_idx: usize) -> Result<bool> {
        // Implementation for borrowing from right sibling
        Ok(false)
    }

    /// Merge with left sibling
    fn merge_with_left(&mut self, _node: &mut BTreeNode, _child_idx: usize) -> Result<bool> {
        // Implementation for merging with left sibling
        Ok(false)
    }

    /// Merge with right sibling
    fn merge_with_right(&mut self, _node: &mut BTreeNode, _child_idx: usize) -> Result<bool> {
        // Implementation for merging with right sibling
        Ok(false)
    }
}

/// B-Tree iterator for range queries
#[derive(Debug)]
pub struct BTreeIterator {
    stack: VecDeque<(PageId, BTreeNode, usize)>,
    buffer_manager: std::sync::Arc<crate::storage::BufferPoolManager>,
}

impl BTreeIterator {
    /// Create a new iterator starting from the first key
    pub fn new(
        btree: &BTree,
    ) -> Result<Self> {
        let mut stack = VecDeque::new();
        let (root_id, root_node) = btree.load_node(btree.root_page_id)?;

        // Traverse to leftmost leaf
        let mut current_id = root_id;
        let mut current_node = root_node;

        while current_node.node_type == BTreeNodeType::Internal {
            stack.push_back((current_id, current_node.clone(), 0));
            current_id = current_node.children[0];
            let (_, node) = btree.load_node(current_id)?;
            current_node = node;
        }

        stack.push_back((current_id, current_node, 0));

        Ok(Self {
            stack,
            buffer_manager: btree.buffer_manager.clone(),
        })
    }

    /// Create a new iterator starting from a specific key
    pub fn seek(
        btree: &BTree,
        _key: &BTreeKey,
    ) -> Result<Self> {
        // Implementation for seeking to a specific key
        Self::new(btree)
    }
}

impl Iterator for BTreeIterator {
    type Item = Result<(BTreeKey, BTreeValue)>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((page_id, mut node, entry_idx)) = self.stack.pop_back() {
            if entry_idx < node.entry_count {
                // Return current entry
                let entry = node.entries[entry_idx].clone();

                // Push back current node with next entry index
                self.stack.push_back((page_id, node, entry_idx + 1));

                return Some(Ok((entry.key, entry.value)));
            } else if node.node_type == BTreeNodeType::Internal {
                // Move to next child
                if entry_idx - 1 < node.children.len() {
                    let child_id = node.children[entry_idx - 1];
                    let file_manager = &self.buffer_manager.file_manager;
                    match file_manager.lock().unwrap().read_page(child_id) {
                        Ok(page) => {
                            let node_size = page.data.len() - page.header.free_bytes;
                            let node_bytes = &page.data[..node_size];

                            match bincode::deserialize::<BTreeNode>(node_bytes) {
                                Ok(child_node) => {
                                    self.stack.push_back((child_id, child_node, 0));
                                }
                                Err(e) => {
                                    return Some(Err(
                                        crate::error::RustgreSQLError::Serialization(e.to_string())
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            return Some(Err(e));
                        }
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{BufferPoolManager, PageType};
    use std::sync::Arc;

    fn create_test_btree() -> Result<(BTree, Arc<BufferPoolManager>)> {
        let file_manager = Arc::new(std::sync::Mutex::new(
            crate::storage::test_utils::MockFileManager::new()
        ));
        let buffer_manager = Arc::new(BufferPoolManager::new(10, file_manager));
        let btree = BTree::new(buffer_manager.clone())?;

        Ok((btree, buffer_manager))
    }

    #[test]
    fn test_btree_node_operations() {
        let mut node = BTreeNode::new(BTreeNodeType::Leaf);

        assert!(node.is_empty());
        assert!(!node.is_full());

        let entry = BTreeEntry {
            key: b"test_key".to_vec(),
            value: 42,
        };

        node.insert_entry(entry.clone()).unwrap();
        assert_eq!(node.entry_count, 1);
        assert_eq!(node.find_key(&entry.key), Some(0));

        let removed = node.remove_entry(&entry.key).unwrap();
        assert_eq!(removed.key, entry.key);
        assert!(node.is_empty());
    }

    #[test]
    fn test_btree_insert_and_search() -> Result<()> {
        let (mut btree, _) = create_test_btree()?;

        let key = b"test_key".to_vec();
        let value = 123;

        // Insert
        btree.insert(key.clone(), value)?;

        // Search
        let found_value = btree.search(&key)?;
        assert_eq!(found_value, Some(value));

        Ok(())
    }

    #[test]
    fn test_btree_delete() -> Result<()> {
        let (mut btree, _) = create_test_btree()?;

        let key = b"test_key".to_vec();
        let value = 123;

        // Insert
        btree.insert(key.clone(), value)?;

        // Delete
        let deleted_value = btree.delete(&key)?;
        assert_eq!(deleted_value, Some(value));

        // Search should return None
        let found_value = btree.search(&key)?;
        assert_eq!(found_value, None);

        Ok(())
    }
}