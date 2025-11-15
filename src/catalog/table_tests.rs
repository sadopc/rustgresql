#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DataType, DataTypeKind};

    #[test]
    fn test_create_table() {
        let catalog = SystemTableManager::new();

        let columns = vec![
            ColumnDef {
                column_id: 0,
                name: "id".to_string(),
                data_type: DataType::new(DataTypeKind::Integer).nullable(false),
                nullable: false,
                default_value: None,
                primary_key: true,
                unique: false,
                check_constraint: None,
            },
            ColumnDef {
                column_id: 1,
                name: "name".to_string(),
                data_type: DataType::new(DataTypeKind::Text).nullable(false),
                nullable: false,
                default_value: None,
                primary_key: false,
                unique: false,
                check_constraint: None,
            },
        ];

        let table_id = catalog.create_table("users", columns).unwrap();
        assert!(table_id > 1); // Should be > 1 since 0 and 1 are system tables

        // Verify table was created
        let table_def = catalog.get_table("users").unwrap().unwrap();
        assert_eq!(table_def.name, "users");
        assert_eq!(table_def.columns.len(), 2);
        assert_eq!(table_def.columns[0].name, "id");
        assert_eq!(table_def.columns[1].name, "name");
    }

    #[test]
    fn test_drop_table() {
        let catalog = SystemTableManager::new();

        let columns = vec![
            ColumnDef {
                column_id: 0,
                name: "id".to_string(),
                data_type: DataType::new(DataTypeKind::Integer),
                nullable: false,
                default_value: None,
                primary_key: true,
                unique: false,
                check_constraint: None,
            },
        ];

        catalog.create_table("temp_table", columns).unwrap();
        assert!(catalog.get_table("temp_table").unwrap().is_some());

        catalog.drop_table("temp_table").unwrap();
        assert!(catalog.get_table("temp_table").unwrap().is_none());
    }

    #[test]
    fn test_list_tables() {
        let catalog = SystemTableManager::new();

        // Add a user table
        let columns = vec![
            ColumnDef {
                column_id: 0,
                name: "id".to_string(),
                data_type: DataType::new(DataTypeKind::Integer),
                nullable: false,
                default_value: None,
                primary_key: false,
                unique: false,
                check_constraint: None,
            },
        ];

        catalog.create_table("test_table", columns).unwrap();

        let tables = catalog.list_tables().unwrap();
        // Should contain only user tables (system tables are filtered out)
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "test_table");
    }

    #[test]
    fn test_table_crud_operations() {
        let catalog = SystemTableManager::new();

        let columns = vec![
            ColumnDef {
                column_id: 0,
                name: "id".to_string(),
                data_type: DataType::new(DataTypeKind::Integer),
                nullable: false,
                default_value: None,
                primary_key: false,
                unique: false,
                check_constraint: None,
            },
            ColumnDef {
                column_id: 1,
                name: "value".to_string(),
                data_type: DataType::new(DataTypeKind::Text),
                nullable: false,
                default_value: None,
                primary_key: false,
                unique: false,
                check_constraint: None,
            },
        ];

        catalog.create_table("data", columns).unwrap();

        // Insert data
        let row1 = vec![
            crate::types::Value { kind: crate::types::ValueKind::Integer(1) },
            crate::types::Value { kind: crate::types::ValueKind::String("hello".to_string()) },
        ];

        let row2 = vec![
            crate::types::Value { kind: crate::types::ValueKind::Integer(2) },
            crate::types::Value { kind: crate::types::ValueKind::String("world".to_string()) },
        ];

        catalog.insert("data", row1).unwrap();
        catalog.insert("data", row2).unwrap();

        // Select data
        let results = catalog.select("data").unwrap();
        assert_eq!(results.len(), 2);

        // Verify first row
        assert_eq!(results[0][0].kind, crate::types::ValueKind::Integer(1));
        assert_eq!(results[0][1].kind, crate::types::ValueKind::String("hello".to_string()));

        // Get column names
        let column_names = catalog.get_column_names("data").unwrap();
        assert_eq!(column_names, vec!["id", "value"]);
    }

    #[test]
    fn test_system_tables_initialization() {
        let catalog = SystemTableManager::new();

        // Check that system tables exist
        assert!(catalog.get_table("pg_table").unwrap().is_some());
        assert!(catalog.get_table("pg_column").unwrap().is_some());

        // Verify system table structure
        let pg_table = catalog.get_table("pg_table").unwrap().unwrap();
        assert_eq!(pg_table.table_id, 0);
        assert_eq!(pg_table.columns.len(), 3); // table_id, table_name, schema_id

        let pg_column = catalog.get_table("pg_column").unwrap().unwrap();
        assert_eq!(pg_column.table_id, 1);
        assert_eq!(pg_column.columns.len(), 3); // column_id, table_id, column_name
    }

    #[test]
    fn test_duplicate_table_creation() {
        let catalog = SystemTableManager::new();

        let columns = vec![
            ColumnDef {
                column_id: 0,
                name: "id".to_string(),
                data_type: DataType::new(DataTypeKind::Integer),
                nullable: false,
                default_value: None,
                primary_key: false,
                unique: false,
                check_constraint: None,
            },
        ];

        catalog.create_table("duplicate_test", columns.clone()).unwrap();

        // Should fail when trying to create the same table again
        let result = catalog.create_table("duplicate_test", columns);
        assert!(result.is_err());
    }
}