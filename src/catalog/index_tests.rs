#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_index() {
        let manager = IndexManager::new();

        let index_id = manager.create_index(
            "test_index",
            1, // table_id
            vec!["column1".to_string(), "column2".to_string()],
            IndexType::BTree,
            false, // not unique
        ).unwrap();

        assert!(index_id > 0);

        let index_info = manager.get_index("test_index").unwrap().unwrap();
        assert_eq!(index_info.def.name, "test_index");
        assert_eq!(index_info.def.table_id, 1);
        assert_eq!(index_info.def.columns, vec!["column1", "column2"]);
        assert_eq!(index_info.def.index_type, IndexType::BTree);
        assert!(!index_info.def.unique);
        assert!(!index_info.def.primary_key);
    }

    #[test]
    fn test_create_unique_index() {
        let manager = IndexManager::new();

        let index_id = manager.create_index(
            "unique_test",
            2,
            vec!["email".to_string()],
            IndexType::BTree,
            true, // unique
        ).unwrap();

        let index_info = manager.get_index("unique_test").unwrap().unwrap();
        assert!(index_info.def.unique);
        assert!(!index_info.def.primary_key);
    }

    #[test]
    fn test_create_primary_key_index() {
        let manager = IndexManager::new();

        let index_id = manager.create_primary_key_index(
            1,
            "users",
            vec!["id".to_string()],
        ).unwrap();

        assert!(index_id > 0);

        let index_info = manager.get_index("pk_users").unwrap().unwrap();
        assert_eq!(index_info.def.name, "pk_users");
        assert!(index_info.def.primary_key);
        assert!(index_info.def.unique);
        assert_eq!(index_info.def.index_type, IndexType::BTree);
    }

    #[test]
    fn test_duplicate_index_creation() {
        let manager = IndexManager::new();

        manager.create_index(
            "duplicate_test",
            1,
            vec!["col".to_string()],
            IndexType::BTree,
            false,
        ).unwrap();

        // Should fail when trying to create the same index again
        let result = manager.create_index(
            "duplicate_test",
            2,
            vec!["col2".to_string()],
            IndexType::Hash,
            true,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_drop_index() {
        let manager = IndexManager::new();

        manager.create_index(
            "drop_test",
            1,
            vec!["column".to_string()],
            IndexType::BTree,
            false,
        ).unwrap();

        assert!(manager.index_exists("drop_test"));
        assert!(manager.get_index("drop_test").unwrap().is_some());

        manager.drop_index("drop_test").unwrap();

        assert!(!manager.index_exists("drop_test"));
        assert!(manager.get_index("drop_test").unwrap().is_none());
    }

    #[test]
    fn test_list_indexes() {
        let manager = IndexManager::new();

        // Create several indexes
        manager.create_index("index1", 1, vec!["col1".to_string()], IndexType::BTree, false).unwrap();
        manager.create_index("index2", 2, vec!["col2".to_string()], IndexType::Hash, true).unwrap();
        manager.create_primary_key_index(3, "table3", vec!["id".to_string()]).unwrap();

        let all_indexes = manager.list_indexes().unwrap();
        assert_eq!(all_indexes.len(), 3);

        let index_names: Vec<String> = all_indexes.iter().map(|i| i.def.name.clone()).collect();
        assert!(index_names.contains(&"index1".to_string()));
        assert!(index_names.contains(&"index2".to_string()));
        assert!(index_names.contains(&"pk_table3".to_string()));
    }

    #[test]
    fn test_list_table_indexes() {
        let manager = IndexManager::new();

        // Create indexes for different tables
        manager.create_index("idx1_tab1", 1, vec!["col1".to_string()], IndexType::BTree, false).unwrap();
        manager.create_index("idx2_tab1", 1, vec!["col2".to_string()], IndexType::Hash, true).unwrap();
        manager.create_index("idx1_tab2", 2, vec!["col3".to_string()], IndexType::BTree, true).unwrap();

        let table1_indexes = manager.list_table_indexes(1).unwrap();
        assert_eq!(table1_indexes.len(), 2);

        let table2_indexes = manager.list_table_indexes(2).unwrap();
        assert_eq!(table2_indexes.len(), 1);

        let table3_indexes = manager.list_table_indexes(3).unwrap();
        assert_eq!(table3_indexes.len(), 0);
    }

    #[test]
    fn test_get_primary_key_index() {
        let manager = IndexManager::new();

        // Create a primary key index
        manager.create_primary_key_index(1, "users", vec!["id".to_string()]).unwrap();

        // Create a regular unique index
        manager.create_index("unique_email", 1, vec!["email".to_string()], IndexType::BTree, true).unwrap();

        let pk_index = manager.get_primary_key_index(1).unwrap().unwrap();
        assert_eq!(pk_index.def.name, "pk_users");
        assert!(pk_index.def.primary_key);

        // Should return None for table without primary key
        let no_pk = manager.get_primary_key_index(2).unwrap();
        assert!(no_pk.is_none());
    }

    #[test]
    fn test_index_types() {
        let manager = IndexManager::new();

        let index_types = vec![
            IndexType::BTree,
            IndexType::Hash,
            IndexType::GIN,
            IndexType::GIST,
        ];

        for (i, index_type) in index_types.iter().enumerate() {
            let index_name = format!("type_test_{}", i);
            manager.create_index(
                &index_name,
                i as u64,
                vec!["col".to_string()],
                index_type.clone(),
                false,
            ).unwrap();

            let index_info = manager.get_index(&index_name).unwrap().unwrap();
            assert_eq!(index_info.def.index_type, *index_type);
        }
    }

    #[test]
    fn test_index_statistics() {
        let manager = IndexManager::new();

        manager.create_index(
            "stats_test",
            1,
            vec!["col".to_string()],
            IndexType::BTree,
            false,
        ).unwrap();

        // Initial stats should be empty
        let initial_stats = manager.get_stats("stats_test").unwrap().unwrap();
        assert_eq!(initial_stats.pages_used, 0);
        assert_eq!(initial_stats.entries, 0);
        assert_eq!(initial_stats.height, 0);
        assert!(initial_stats.last_analyzed.is_none());

        // Update stats
        let new_stats = IndexStats {
            pages_used: 100,
            entries: 1000,
            height: 3,
            last_analyzed: Some(std::time::SystemTime::now()),
        };

        manager.update_stats("stats_test", new_stats.clone()).unwrap();

        // Verify updated stats
        let updated_stats = manager.get_stats("stats_test").unwrap().unwrap();
        assert_eq!(updated_stats.pages_used, 100);
        assert_eq!(updated_stats.entries, 1000);
        assert_eq!(updated_stats.height, 3);
        assert!(updated_stats.last_analyzed.is_some());
    }

    #[test]
    fn test_get_index_by_id() {
        let manager = IndexManager::new();

        let index_id = manager.create_index(
            "by_id_test",
            1,
            vec!["col".to_string()],
            IndexType::BTree,
            false,
        ).unwrap();

        let index_by_id = manager.get_index_by_id(index_id).unwrap().unwrap();
        let index_by_name = manager.get_index("by_id_test").unwrap().unwrap();

        assert_eq!(index_by_id.def.index_id, index_by_name.def.index_id);
        assert_eq!(index_by_id.def.name, index_by_name.def.name);
        assert_eq!(index_by_id.def.table_id, index_by_name.def.table_id);
    }

    #[test]
    fn test_index_timestamps() {
        let manager = IndexManager::new();

        let before_creation = std::time::SystemTime::now();
        let index_id = manager.create_index(
            "timestamp_test",
            1,
            vec!["col".to_string()],
            IndexType::BTree,
            false,
        ).unwrap();
        let after_creation = std::time::SystemTime::now();

        let index_info = manager.get_index("timestamp_test").unwrap().unwrap();

        // Verify timestamps are set correctly
        assert!(index_info.def.created_at >= before_creation);
        assert!(index_info.def.created_at <= after_creation);
        assert!(index_info.def.modified_at >= before_creation);
        assert!(index_info.def.modified_at <= after_creation);
        assert_eq!(index_info.def.created_at, index_info.def.modified_at);
    }
}