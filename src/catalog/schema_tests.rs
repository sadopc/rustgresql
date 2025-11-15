#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_creation() {
        let manager = SchemaManager::new();

        let schema_id = manager.create_schema("test_schema", 100).unwrap();
        assert!(schema_id > 1); // Should be > 1 since 0 and 1 are system schemas

        let schema_def = manager.get_schema("test_schema").unwrap().unwrap();
        assert_eq!(schema_def.name, "test_schema");
        assert_eq!(schema_def.owner_id, 100);
        assert_eq!(schema_def.schema_id, schema_id);
    }

    #[test]
    fn test_default_schemas() {
        let manager = SchemaManager::new();

        // Check that default schemas exist
        let public_schema = manager.get_schema("public").unwrap().unwrap();
        assert_eq!(public_schema.name, "public");
        assert_eq!(public_schema.owner_id, 1);
        assert_eq!(public_schema.schema_id, 1);

        let catalog_schema = manager.get_schema("pg_catalog").unwrap().unwrap();
        assert_eq!(catalog_schema.name, "pg_catalog");
        assert_eq!(catalog_schema.owner_id, 0);
        assert_eq!(catalog_schema.schema_id, 0);
    }

    #[test]
    fn test_duplicate_schema_creation() {
        let manager = SchemaManager::new();

        manager.create_schema("duplicate_test", 100).unwrap();

        // Should fail when trying to create the same schema again
        let result = manager.create_schema("duplicate_test", 200);
        assert!(result.is_err());
    }

    #[test]
    fn test_drop_schema() {
        let manager = SchemaManager::new();

        let schema_id = manager.create_schema("drop_test", 100).unwrap();
        assert!(manager.get_schema("drop_test").unwrap().is_some());

        manager.drop_schema("drop_test", false).unwrap();
        assert!(manager.get_schema("drop_test").unwrap().is_none());
    }

    #[test]
    fn test_drop_system_schemas() {
        let manager = SchemaManager::new();

        // Should not be able to drop system schemas without cascade
        let result1 = manager.drop_schema("pg_catalog", false);
        assert!(result1.is_err());

        let result2 = manager.drop_schema("public", false);
        assert!(result2.is_err());

        // Even with cascade, pg_catalog should not be droppable
        let result3 = manager.drop_schema("pg_catalog", true);
        assert!(result3.is_err());

        // But public should be droppable with cascade
        let result4 = manager.drop_schema("public", true);
        assert!(result4.is_ok());
    }

    #[test]
    fn test_list_schemas() {
        let manager = SchemaManager::new();

        // Initially should have at least the default schemas
        let schemas = manager.list_schemas().unwrap();
        let schema_names: Vec<String> = schemas.iter().map(|s| s.name.clone()).collect();
        assert!(schema_names.contains(&"public".to_string()));
        assert!(schema_names.contains(&"pg_catalog".to_string()));

        // Add a new schema
        manager.create_schema("list_test", 100).unwrap();

        let updated_schemas = manager.list_schemas().unwrap();
        let updated_names: Vec<String> = updated_schemas.iter().map(|s| s.name.clone()).collect();
        assert!(updated_names.contains(&"list_test".to_string()));
    }

    #[test]
    fn test_schema_by_id() {
        let manager = SchemaManager::new();

        let schema_id = manager.create_schema("by_id_test", 100).unwrap();

        let schema_by_id = manager.get_schema_by_id(schema_id).unwrap().unwrap();
        let schema_by_name = manager.get_schema("by_id_test").unwrap().unwrap();

        assert_eq!(schema_by_id.name, schema_by_name.name);
        assert_eq!(schema_by_id.owner_id, schema_by_name.owner_id);
        assert_eq!(schema_by_id.schema_id, schema_by_name.schema_id);
    }

    #[test]
    fn test_schema_exists() {
        let manager = SchemaManager::new();

        assert!(manager.schema_exists("public"));
        assert!(manager.schema_exists("pg_catalog"));
        assert!(!manager.schema_exists("nonexistent"));

        manager.create_schema("exists_test", 100).unwrap();
        assert!(manager.schema_exists("exists_test"));
    }

    #[test]
    fn test_get_special_schema_ids() {
        let manager = SchemaManager::new();

        assert_eq!(manager.get_public_schema_id(), 1);
        assert_eq!(manager.get_system_schema_id(), 0);
    }

    #[test]
    fn test_schema_timestamps() {
        let manager = SchemaManager::new();

        let before_creation = std::time::SystemTime::now();
        let schema_id = manager.create_schema("timestamp_test", 100).unwrap();
        let after_creation = std::time::SystemTime::now();

        let schema_def = manager.get_schema("timestamp_test").unwrap().unwrap();

        // Verify timestamps are set correctly
        assert!(schema_def.created_at >= before_creation);
        assert!(schema_def.created_at <= after_creation);
        assert!(schema_def.modified_at >= before_creation);
        assert!(schema_def.modified_at <= after_creation);
        assert_eq!(schema_def.created_at, schema_def.modified_at);
    }
}