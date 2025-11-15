//! Integration tests for the complete database system

use rustgresql::*;

#[test]
fn test_full_database_workflow() {
    // Initialize the database
    let config = Config {
        page_size: 8192,
        buffer_pool_size: 1000,
        wal_enabled: true,
        wal_file_path: Some("test_wal.log".to_string()),
        data_file_path: "test_data.db".to_string(),
    };

    let db = Database::new(config).unwrap();

    // Start a transaction
    let mut tx = db.begin_transaction().unwrap();

    // Create a table
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
        ColumnDef {
            column_id: 1,
            name: "name".to_string(),
            data_type: DataType::new(DataTypeKind::Varchar(100)),
            nullable: false,
            default_value: None,
            primary_key: false,
            unique: false,
            check_constraint: None,
        },
        ColumnDef {
            column_id: 2,
            name: "email".to_string(),
            data_type: DataType::new(DataTypeKind::Text),
            nullable: true,
            default_value: None,
            primary_key: false,
            unique: true,
            check_constraint: None,
        },
    ];

    let table_id = tx.create_table("users", columns).unwrap();
    assert!(table_id > 0);

    // Insert data
    let user1 = vec![
        Value { kind: ValueKind::Integer(1) },
        Value { kind: ValueKind::String("Alice".to_string()) },
        Value { kind: ValueKind::String("alice@example.com".to_string()) },
    ];

    let user2 = vec![
        Value { kind: ValueKind::Integer(2) },
        Value { kind: ValueKind::String("Bob".to_string()) },
        Value { kind: ValueKind::String("bob@example.com".to_string()) },
    ];

    tx.insert("users", user1).unwrap();
    tx.insert("users", user2).unwrap();

    // Commit the transaction
    tx.commit().unwrap();

    // Start a new transaction to verify data
    let mut read_tx = db.begin_transaction().unwrap();
    let results = read_tx.select("users").unwrap();

    assert_eq!(results.len(), 2);

    // Verify the data
    let alice_row = &results[0];
    assert_eq!(alice_row[0].kind, ValueKind::Integer(1));
    assert_eq!(alice_row[1].kind, ValueKind::String("Alice".to_string()));
    assert_eq!(alice_row[2].kind, ValueKind::String("alice@example.com".to_string()));

    read_tx.rollback().unwrap();
}

#[test]
fn test_sql_parsing_and_execution() {
    let config = Config {
        page_size: 8192,
        buffer_pool_size: 1000,
        wal_enabled: true,
        wal_file_path: Some("test_sql_wal.log".to_string()),
        data_file_path: "test_sql_data.db".to_string(),
    };

    let db = Database::new(config).unwrap();

    // Test SQL parsing
    let sql = "CREATE TABLE users (id INTEGER PRIMARY KEY, name VARCHAR(100) NOT NULL, email TEXT UNIQUE)";
    let statements = parse_sql(sql).unwrap();

    assert_eq!(statements.len(), 1);
    match &statements[0] {
        Statement::CreateTable(create_table) => {
            assert_eq!(create_table.table.name, "users");
            assert_eq!(create_table.columns.len(), 3);
            assert_eq!(create_table.columns[0].name, "id");
            assert_eq!(create_table.columns[1].name, "name");
            assert_eq!(create_table.columns[2].name, "email");
        }
        _ => panic!("Expected CreateTable statement"),
    }

    // Execute the CREATE TABLE statement
    let mut tx = db.begin_transaction().unwrap();
    for statement in &statements {
        tx.execute_statement(statement).unwrap();
    }
    tx.commit().unwrap();

    // Verify the table was created
    let catalog = get_catalog();
    let table_def = catalog.get_table("users").unwrap().unwrap();
    assert_eq!(table_def.name, "users");
    assert_eq!(table_def.columns.len(), 3);

    // Verify primary key index was created
    let indexes = catalog.index_manager.list_table_indexes(table_def.table_id).unwrap();
    assert_eq!(indexes.len(), 1);
    assert!(indexes[0].def.primary_key);
    assert_eq!(indexes[0].def.name, "pk_users");
}

#[test]
fn test_transaction_rollback() {
    let config = Config {
        page_size: 8192,
        buffer_pool_size: 1000,
        wal_enabled: true,
        wal_file_path: Some("test_rollback_wal.log".to_string()),
        data_file_path: "test_rollback_data.db".to_string(),
    };

    let db = Database::new(config).unwrap();

    // Create table
    let mut create_tx = db.begin_transaction().unwrap();
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
        ColumnDef {
            column_id: 1,
            name: "data".to_string(),
            data_type: DataType::new(DataTypeKind::Text),
            nullable: false,
            default_value: None,
            primary_key: false,
            unique: false,
            check_constraint: None,
        },
    ];

    create_tx.create_table("rollback_test", columns).unwrap();
    create_tx.commit().unwrap();

    // Start transaction that will be rolled back
    let mut rollback_tx = db.begin_transaction().unwrap();

    let test_data = vec![
        Value { kind: ValueKind::Integer(1) },
        Value { kind: ValueKind::String("test".to_string()) },
    ];

    rollback_tx.insert("rollback_test", test_data).unwrap();
    rollback_tx.rollback().unwrap();

    // Verify data was not committed
    let mut verify_tx = db.begin_transaction().unwrap();
    let results = verify_tx.select("rollback_test").unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_multi_transaction_isolation() {
    let config = Config {
        page_size: 8192,
        buffer_pool_size: 1000,
        wal_enabled: true,
        wal_file_path: Some("test_isolation_wal.log".to_string()),
        data_file_path: "test_isolation_data.db".to_string(),
    };

    let db = Database::new(config).unwrap();

    // Create table
    let mut create_tx = db.begin_transaction().unwrap();
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

    create_tx.create_table("isolation_test", columns).unwrap();
    create_tx.commit().unwrap();

    // Start two concurrent transactions
    let mut tx1 = db.begin_transaction().unwrap();
    let mut tx2 = db.begin_transaction().unwrap();

    // tx1 inserts data
    let data1 = vec![
        Value { kind: ValueKind::Integer(1) },
        Value { kind: ValueKind::String("from_tx1".to_string()) },
    ];

    tx1.insert("isolation_test", data1).unwrap();

    // tx2 should not see uncommitted data from tx1
    let results2 = tx2.select("isolation_test").unwrap();
    assert_eq!(results2.len(), 0);

    // tx1 commits
    tx1.commit().unwrap();

    // tx2 still should not see committed data due to snapshot isolation
    let results2_after = tx2.select("isolation_test").unwrap();
    assert_eq!(results2_after.len(), 0);

    // tx2 commits and starts new transaction
    tx2.commit().unwrap();
    let mut tx3 = db.begin_transaction().unwrap();

    // tx3 should now see the committed data
    let results3 = tx3.select("isolation_test").unwrap();
    assert_eq!(results3.len(), 1);
    assert_eq!(results3[0][1].kind, ValueKind::String("from_tx1".to_string()));

    tx3.commit().unwrap();
}

#[test]
fn test_btree_operations() {
    use std::sync::Arc;

    let config = Config {
        page_size: 8192,
        buffer_pool_size: 1000,
        wal_enabled: true,
        wal_file_path: Some("test_btree_wal.log".to_string()),
        data_file_path: "test_btree_data.db".to_string(),
    };

    let db = Database::new(config).unwrap();
    let buffer_manager = Arc::new(storage::BufferPoolManager::new(
        8192,
        1000,
        "test_btree_buffer.db".to_string(),
    ));

    let btree = storage::BTree::new(buffer_manager);

    // Test basic B-Tree operations
    let key1 = storage::BTreeKey::Integer(1);
    let value1 = storage::BTreeValue::Raw(b"test_value_1".to_vec());

    let key2 = storage::BTreeKey::Integer(2);
    let value2 = storage::BTreeValue::Raw(b"test_value_2".to_vec());

    // Insert values
    let mut tx = db.begin_transaction().unwrap();
    btree.insert(&mut tx, &key1, &value1).unwrap();
    btree.insert(&mut tx, &key2, &value2).unwrap();
    tx.commit().unwrap();

    // Search for values
    let mut search_tx = db.begin_transaction().unwrap();
    let found1 = btree.search(&mut search_tx, &key1).unwrap();
    let found2 = btree.search(&mut search_tx, &key2).unwrap();

    assert!(found1.is_some());
    assert!(found2.is_some());

    match found1.unwrap() {
        storage::BTreeValue::Raw(data) => assert_eq!(data, b"test_value_1"),
        _ => panic!("Expected Raw value"),
    }

    search_tx.rollback().unwrap();
}

#[test]
fn test_comprehensive_type_system() {
    use crate::types::parse_data_type;

    // Test parsing all supported types
    let type_tests = vec![
        ("INTEGER", "integer"),
        ("VARCHAR(255)", "varchar(255)"),
        ("TEXT", "text"),
        ("BOOLEAN", "boolean"),
        ("TIMESTAMP", "timestamp"),
        ("DATE", "date"),
        ("NUMERIC(10,2)", "numeric(10, 2)"),
        ("SERIAL", "serial"),
        ("UUID", "uuid"),
        ("JSONB", "jsonb"),
    ];

    for (input, expected_output) in type_tests {
        let parsed_type = parse_data_type(input).unwrap();
        assert_eq!(parsed_type.type_name(), expected_output);
    }
}

#[test]
fn test_catalog_integration() {
    let catalog = get_catalog();

    // Test creating table with catalog
    let columns = vec![
        ColumnDef {
            column_id: 0,
            name: "id".to_string(),
            data_type: DataType::new(DataTypeKind::Serial),
            nullable: false,
            default_value: None,
            primary_key: true,
            unique: false,
            check_constraint: None,
        },
        ColumnDef {
            column_id: 1,
            name: "name".to_string(),
            data_type: DataType::new(DataTypeKind::Varchar(100)),
            nullable: false,
            default_value: None,
            primary_key: false,
            unique: false,
            check_constraint: None,
        },
    ];

    let table_id = catalog.create_table("catalog_test", columns).unwrap();
    assert!(table_id > 0);

    // Verify table exists
    let table_def = catalog.get_table("catalog_test").unwrap().unwrap();
    assert_eq!(table_def.name, "catalog_test");

    // Verify primary key index was automatically created
    let pk_index = catalog.index_manager.get_primary_key_index(table_id).unwrap().unwrap();
    assert!(pk_index.def.primary_key);
    assert_eq!(pk_index.def.columns, vec!["id"]);

    // Test getting table with indexes
    let (table_with_indexes, indexes) = catalog.get_table_with_indexes("catalog_test").unwrap().unwrap();
    assert_eq!(table_with_indexes.table_id, table_id);
    assert_eq!(indexes.len(), 1);
    assert!(indexes[0].def.primary_key);

    // Test catalog statistics
    let stats = catalog.get_catalog_stats().unwrap();
    assert!(stats.table_count > 0);
    assert!(stats.schema_count >= 2); // pg_catalog and public
    assert!(stats.index_count > 0);
}