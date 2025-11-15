#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::lexer::*;

    #[test]
    fn test_parse_simple_select() {
        let sql = "SELECT id, name FROM users";
        let statements = parse_sql(sql).unwrap();

        assert_eq!(statements.len(), 1);

        match &statements[0] {
            Statement::Select(select) => {
                assert_eq!(select.columns.len(), 2);
                assert!(matches!(select.columns[0], Expression::Column { name, .. } if name == "id"));
                assert!(matches!(select.columns[1], Expression::Column { name, .. } if name == "name"));
                assert_eq!(select.from.len(), 1);
                assert_eq!(select.from[0].name, "users");
            }
            _ => panic!("Expected SELECT statement"),
        }
    }

    #[test]
    fn test_parse_select_with_where() {
        let sql = "SELECT * FROM users WHERE age > 18";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::Select(select) => {
                assert!(matches!(select.columns[0], Expression::Star));
                assert!(select.where_clause.is_some());
            }
            _ => panic!("Expected SELECT statement"),
        }
    }

    #[test]
    fn test_parse_create_table() {
        let sql = "CREATE TABLE users (id INTEGER PRIMARY KEY, name VARCHAR(100) NOT NULL)";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::CreateTable(create_table) => {
                assert_eq!(create_table.table_name, "users");
                assert_eq!(create_table.columns.len(), 2);
                assert_eq!(create_table.table_constraints.len(), 0);

                // Check first column (id)
                let id_column = &create_table.columns[0];
                assert_eq!(id_column.name, "id");
                assert_eq!(id_column.data_type.kind, DataTypeKind::Integer);

                // Check constraints
                assert_eq!(id_column.constraints.len(), 1);
                assert!(matches!(id_column.constraints[0], ColumnConstraint::PrimaryKey));

                // Check second column (name)
                let name_column = &create_table.columns[1];
                assert_eq!(name_column.name, "name");
                assert_eq!(name_column.data_type.kind, DataTypeKind::Varchar(100));

                // Check constraints
                assert_eq!(name_column.constraints.len(), 1);
                assert!(matches!(name_column.constraints[0], ColumnConstraint::NotNull));
            }
            _ => panic!("Expected CREATE TABLE statement"),
        }
    }

    #[test]
    fn test_parse_create_table_with_table_constraints() {
        let sql = "CREATE TABLE users (
            id INTEGER,
            email VARCHAR(255),
            PRIMARY KEY (id),
            UNIQUE (email),
            CHECK (id > 0)
        )";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::CreateTable(create_table) => {
                assert_eq!(create_table.table_name, "users");
                assert_eq!(create_table.columns.len(), 2);
                assert_eq!(create_table.table_constraints.len(), 3);

                // Check first column (id)
                let id_column = &create_table.columns[0];
                assert_eq!(id_column.name, "id");
                assert_eq!(id_column.data_type.kind, DataTypeKind::Integer);
                assert_eq!(id_column.constraints.len(), 0);

                // Check second column (email)
                let email_column = &create_table.columns[1];
                assert_eq!(email_column.name, "email");
                assert_eq!(email_column.data_type.kind, DataTypeKind::Varchar(255));
                assert_eq!(email_column.constraints.len(), 0);

                // Check table constraints
                assert!(matches!(
                    &create_table.table_constraints[0],
                    TableConstraint::PrimaryKey { columns, .. } if columns == &["id".to_string()]
                ));

                assert!(matches!(
                    &create_table.table_constraints[1],
                    TableConstraint::Unique { columns, .. } if columns == &["email".to_string()]
                ));

                assert!(matches!(
                    &create_table.table_constraints[2],
                    TableConstraint::Check { .. }
                ));
            }
            _ => panic!("Expected CREATE TABLE statement"),
        }
    }

    #[test]
    fn test_parse_drop_table() {
        let sql = "DROP TABLE users";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::DropTable(drop_table) => {
                assert_eq!(drop_table.table_name, "users");
                assert_eq!(drop_table.if_exists, false);
            }
            _ => panic!("Expected DROP TABLE statement"),
        }
    }

    #[test]
    fn test_parse_drop_table_if_exists() {
        let sql = "DROP TABLE IF EXISTS users";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::DropTable(drop_table) => {
                assert_eq!(drop_table.table_name, "users");
                assert_eq!(drop_table.if_exists, true);
            }
            _ => panic!("Expected DROP TABLE statement"),
        }
    }

    #[test]
    fn test_parse_drop_index() {
        let sql = "DROP INDEX idx_users_email";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::DropIndex(drop_index) => {
                assert_eq!(drop_index.index_name, "idx_users_email");
                assert_eq!(drop_index.if_exists, false);
            }
            _ => panic!("Expected DROP INDEX statement"),
        }
    }

    #[test]
    fn test_parse_drop_index_if_exists() {
        let sql = "DROP INDEX IF EXISTS idx_users_email";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::DropIndex(drop_index) => {
                assert_eq!(drop_index.index_name, "idx_users_email");
                assert_eq!(drop_index.if_exists, true);
            }
            _ => panic!("Expected DROP INDEX statement"),
        }
    }

    #[test]
    fn test_parse_alter_table_add_column() {
        let sql = "ALTER TABLE users ADD COLUMN email VARCHAR(255)";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::AlterTable(alter_table) => {
                assert_eq!(alter_table.table_name, "users");
                match &alter_table.operation {
                    AlterOperation::AddColumn { column } => {
                        assert_eq!(column.name, "email");
                        assert_eq!(column.data_type.kind, DataTypeKind::Varchar(255));
                    }
                    _ => panic!("Expected ADD COLUMN operation"),
                }
            }
            _ => panic!("Expected ALTER TABLE statement"),
        }
    }

    #[test]
    fn test_parse_alter_table_drop_column() {
        let sql = "ALTER TABLE users DROP COLUMN email";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::AlterTable(alter_table) => {
                assert_eq!(alter_table.table_name, "users");
                match &alter_table.operation {
                    AlterOperation::DropColumn { column_name } => {
                        assert_eq!(column_name, "email");
                    }
                    _ => panic!("Expected DROP COLUMN operation"),
                }
            }
            _ => panic!("Expected ALTER TABLE statement"),
        }
    }

    #[test]
    fn test_parse_alter_table_rename_column() {
        let sql = "ALTER TABLE users RENAME COLUMN email TO user_email";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::AlterTable(alter_table) => {
                assert_eq!(alter_table.table_name, "users");
                match &alter_table.operation {
                    AlterOperation::RenameColumn { old_name, new_name } => {
                        assert_eq!(old_name, "email");
                        assert_eq!(new_name, "user_email");
                    }
                    _ => panic!("Expected RENAME COLUMN operation"),
                }
            }
            _ => panic!("Expected ALTER TABLE statement"),
        }
    }

    #[test]
    fn test_parse_alter_table_rename_table() {
        let sql = "ALTER TABLE users RENAME TO accounts";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::AlterTable(alter_table) => {
                assert_eq!(alter_table.table_name, "users");
                match &alter_table.operation {
                    AlterOperation::RenameTable { new_name } => {
                        assert_eq!(new_name, "accounts");
                    }
                    _ => panic!("Expected RENAME TO operation"),
                }
            }
            _ => panic!("Expected ALTER TABLE statement"),
        }
    }

    #[test]
    fn test_parse_alter_table_add_constraint() {
        let sql = "ALTER TABLE users ADD CONSTRAINT pk_users PRIMARY KEY (id)";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::AlterTable(alter_table) => {
                assert_eq!(alter_table.table_name, "users");
                match &alter_table.operation {
                    AlterOperation::AddConstraint { constraint } => {
                        match constraint {
                            TableConstraint::PrimaryKey { columns, name } => {
                                assert_eq!(columns, &["id".to_string()]);
                                assert_eq!(name, &Some("pk_users".to_string()));
                            }
                            _ => panic!("Expected PRIMARY KEY constraint"),
                        }
                    }
                    _ => panic!("Expected ADD CONSTRAINT operation"),
                }
            }
            _ => panic!("Expected ALTER TABLE statement"),
        }
    }

    #[test]
    fn test_parse_alter_table_drop_constraint() {
        let sql = "ALTER TABLE users DROP CONSTRAINT pk_users";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::AlterTable(alter_table) => {
                assert_eq!(alter_table.table_name, "users");
                match &alter_table.operation {
                    AlterOperation::DropConstraint { constraint_name } => {
                        assert_eq!(constraint_name, "pk_users");
                    }
                    _ => panic!("Expected DROP CONSTRAINT operation"),
                }
            }
            _ => panic!("Expected ALTER TABLE statement"),
        }
    }

    #[test]
    fn test_parse_column_constraints() {
        let sql = "CREATE TABLE products (
            id INTEGER PRIMARY KEY DEFAULT 1,
            name VARCHAR(100) NOT NULL UNIQUE,
            price DECIMAL(10,2) CHECK (price > 0),
            category_id INTEGER REFERENCES categories(id)
        )";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::CreateTable(create_table) => {
                assert_eq!(create_table.columns.len(), 4);

                // Check id column
                let id_column = &create_table.columns[0];
                assert_eq!(id_column.constraints.len(), 2);
                assert!(matches!(id_column.constraints[0], ColumnConstraint::PrimaryKey));
                assert!(matches!(id_column.constraints[1], ColumnConstraint::Default(s) if s == "1"));

                // Check name column
                let name_column = &create_table.columns[1];
                assert_eq!(name_column.constraints.len(), 2);
                assert!(matches!(name_column.constraints[0], ColumnConstraint::NotNull));
                assert!(matches!(name_column.constraints[1], ColumnConstraint::Unique));

                // Check price column
                let price_column = &create_table.columns[2];
                assert_eq!(price_column.constraints.len(), 1);
                assert!(matches!(price_column.constraints[0], ColumnConstraint::Check(_)));

                // Check category_id column
                let category_column = &create_table.columns[3];
                assert_eq!(category_column.constraints.len(), 1);
                assert!(matches!(
                    category_column.constraints[0],
                    ColumnConstraint::References { table, column } if table == "categories" && column == Some("id".to_string())
                ));
            }
            _ => panic!("Expected CREATE TABLE statement"),
        }
    }

    #[test]
    fn test_parse_foreign_key_constraint() {
        let sql = "CREATE TABLE orders (
            id INTEGER PRIMARY KEY,
            user_id INTEGER,
            product_id INTEGER,
            FOREIGN KEY (user_id) REFERENCES users(id),
            FOREIGN KEY (product_id) REFERENCES products(id)
        )";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::CreateTable(create_table) => {
                assert_eq!(create_table.columns.len(), 3);
                assert_eq!(create_table.table_constraints.len(), 2);

                // Check foreign key constraints
                assert!(matches!(
                    &create_table.table_constraints[0],
                    TableConstraint::ForeignKey { columns, ref_table, ref_columns, .. }
                    if columns == &["user_id".to_string()] && ref_table == "users" && ref_columns == &["id".to_string()]
                ));

                assert!(matches!(
                    &create_table.table_constraints[1],
                    TableConstraint::ForeignKey { columns, ref_table, ref_columns, .. }
                    if columns == &["product_id".to_string()] && ref_table == "products" && ref_columns == &["id".to_string()]
                ));
            }
            _ => panic!("Expected CREATE TABLE statement"),
        }
    }

    #[test]
    fn test_parse_insert() {
        let sql = "INSERT INTO users (id, name) VALUES (1, 'Alice')";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::Insert(insert) => {
                assert_eq!(insert.table.name, "users");
                assert_eq!(insert.columns, vec!["id", "name"]);
                assert_eq!(insert.values.len(), 1);
                assert_eq!(insert.values[0].len(), 2);

                // Check values
                match &insert.values[0][0] {
                    Expression::Value(value) => {
                        assert!(matches!(value.kind, ValueKind::Integer(1)));
                    }
                    _ => panic!("Expected integer value"),
                }

                match &insert.values[0][1] {
                    Expression::Value(value) => {
                        assert!(matches!(value.kind, ValueKind::String(s) if s == "Alice"));
                    }
                    _ => panic!("Expected string value"),
                }
            }
            _ => panic!("Expected INSERT statement"),
        }
    }

    #[test]
    fn test_parse_update() {
        let sql = "UPDATE users SET name = 'Bob' WHERE id = 1";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::Update(update) => {
                assert_eq!(update.table.name, "users");
                assert_eq!(update.assignments.len(), 1);
                assert_eq!(update.assignments[0].0, "name");
                assert!(update.where_clause.is_some());
            }
            _ => panic!("Expected UPDATE statement"),
        }
    }

    #[test]
    fn test_parse_delete() {
        let sql = "DELETE FROM users WHERE id = 1";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::Delete(delete) => {
                assert_eq!(delete.table.name, "users");
                assert!(delete.where_clause.is_some());
            }
            _ => panic!("Expected DELETE statement"),
        }
    }

    #[test]
    fn test_parse_create_index() {
        let sql = "CREATE INDEX idx_email ON users (email)";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::CreateIndex(create_index) => {
                assert_eq!(create_index.index.name, "idx_email");
                assert_eq!(create_index.table.name, "users");
                assert_eq!(create_index.columns.len(), 1);
                assert_eq!(create_index.columns[0], "email");
            }
            _ => panic!("Expected CREATE INDEX statement"),
        }
    }

    #[test]
    fn test_parse_select_with_join() {
        let sql = "SELECT u.name, p.title FROM users u JOIN posts p ON u.id = p.user_id";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::Select(select) => {
                assert_eq!(select.columns.len(), 2);
                assert_eq!(select.from.len(), 1);
                assert_eq!(select.joins.len(), 1);

                let join = &select.joins[0];
                assert_eq!(join.table.name, "posts");
                assert_eq!(join.join_type, JoinType::Inner);
                assert!(join.condition.is_some());
            }
            _ => panic!("Expected SELECT statement"),
        }
    }

    #[test]
    fn test_parse_complex_expressions() {
        let sql = "SELECT a + b * c FROM table1 WHERE (x = 1 OR y = 2) AND z > 10";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::Select(select) => {
                // Check arithmetic expression in SELECT clause
                match &select.columns[0] {
                    Expression::BinaryOp { left, op, right } => {
                        assert!(matches!(op, BinaryOperator::Plus));
                        assert!(matches!(left.as_ref(), Expression::Column { name, .. } if name == "a"));
                        match right.as_ref() {
                            Expression::BinaryOp { left: b_left, op: b_op, right: b_right } => {
                                assert!(matches!(b_op, BinaryOperator::Multiply));
                                assert!(matches!(b_left.as_ref(), Expression::Column { name, .. } if name == "b"));
                                assert!(matches!(b_right.as_ref(), Expression::Column { name, .. } if name == "c"));
                            }
                            _ => panic!("Expected multiplication expression"),
                        }
                    }
                    _ => panic!("Expected binary operation"),
                }

                // Check complex WHERE clause
                assert!(select.where_clause.is_some());
            }
            _ => panic!("Expected SELECT statement"),
        }
    }

    #[test]
    fn test_parse_multiple_statements() {
        let sql = "CREATE TABLE test (id INTEGER); INSERT INTO test VALUES (1); SELECT * FROM test;";
        let statements = parse_sql(sql).unwrap();

        assert_eq!(statements.len(), 3);

        // First statement: CREATE TABLE
        assert!(matches!(&statements[0], Statement::CreateTable(_)));

        // Second statement: INSERT
        assert!(matches!(&statements[1], Statement::Insert(_)));

        // Third statement: SELECT
        assert!(matches!(&statements[2], Statement::Select(_)));
    }

    #[test]
    fn test_parse_keywords_case_insensitive() {
        let test_cases = vec![
            ("select * from users", "SELECT"),
            ("INSERT INTO table VALUES (1)", "INSERT"),
            ("update table set col = 1", "UPDATE"),
            ("delete from table", "DELETE"),
            ("create table test (id integer)", "CREATE"),
        ];

        for (sql, expected_type) in test_cases {
            let statements = parse_sql(sql).unwrap();
            assert_eq!(statements.len(), 1);

            match expected_type {
                "SELECT" => assert!(matches!(&statements[0], Statement::Select(_))),
                "INSERT" => assert!(matches!(&statements[0], Statement::Insert(_))),
                "UPDATE" => assert!(matches!(&statements[0], Statement::Update(_))),
                "DELETE" => assert!(matches!(&statements[0], Statement::Delete(_))),
                "CREATE" => assert!(matches!(&statements[0], Statement::CreateTable(_) | Statement::CreateIndex(_))),
                _ => panic!("Unexpected expected_type: {}", expected_type),
            }
        }
    }

    #[test]
    fn test_parse_literals() {
        let sql = "SELECT 42, 3.14, 'hello', TRUE, NULL";
        let statements = parse_sql(sql).unwrap();

        match &statements[0] {
            Statement::Select(select) => {
                assert_eq!(select.columns.len(), 5);

                // Integer literal
                match &select.columns[0] {
                    Expression::Value(value) => {
                        assert!(matches!(value.kind, ValueKind::Integer(42)));
                    }
                    _ => panic!("Expected integer value"),
                }

                // Float literal
                match &select.columns[1] {
                    Expression::Value(value) => {
                        assert!(matches!(value.kind, ValueKind::Float(3.14)));
                    }
                    _ => panic!("Expected float value"),
                }

                // String literal
                match &select.columns[2] {
                    Expression::Value(value) => {
                        assert!(matches!(value.kind, ValueKind::String(s) if s == "hello"));
                    }
                    _ => panic!("Expected string value"),
                }

                // Boolean literal
                match &select.columns[3] {
                    Expression::Value(value) => {
                        assert!(matches!(value.kind, ValueKind::Boolean(true)));
                    }
                    _ => panic!("Expected boolean value"),
                }

                // NULL literal
                match &select.columns[4] {
                    Expression::Value(value) => {
                        assert!(matches!(value.kind, ValueKind::Null(_)));
                    }
                    _ => panic!("Expected null value"),
                }
            }
            _ => panic!("Expected SELECT statement"),
        }
    }

    #[test]
    fn test_parse_invalid_sql() {
        let invalid_statements = vec![
            "SELECT FROM users", // Missing columns
            "INSERT INTO", // Missing table name
            "CREATE TABLE", // Missing table definition
            "UPDATE SET col = 1", // Missing table name
            "DELETE FROM", // Missing table name
            "SELECT * FROM WHERE id = 1", // Missing table name before WHERE
        ];

        for sql in invalid_statements {
            let result = parse_sql(sql);
            assert!(result.is_err(), "Expected parse error for: {}", sql);
        }
    }

    #[test]
    fn test_lexer_functionality() {
        let input = "SELECT id, name FROM users WHERE id = 1;";
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize().unwrap();

        // Expected tokens: SELECT, id, ,, name, FROM, users, WHERE, id, =, 1, ;
        assert_eq!(tokens.len(), 12);

        assert!(matches!(tokens[0].token_type, TokenType::Select));
        assert!(matches!(tokens[1].token_type, TokenType::Identifier(name) if name == "id"));
        assert!(matches!(tokens[2].token_type, TokenType::Comma));
        assert!(matches!(tokens[3].token_type, TokenType::Identifier(name) if name == "name"));
        assert!(matches!(tokens[4].token_type, TokenType::From));
        assert!(matches!(tokens[5].token_type, TokenType::Identifier(name) if name == "users"));
        assert!(matches!(tokens[6].token_type, TokenType::Where));
        assert!(matches!(tokens[7].token_type, TokenType::Identifier(name) if name == "id"));
        assert!(matches!(tokens[8].token_type, TokenType::Equals));
        assert!(matches!(tokens[9].token_type, TokenType::Number(n) if n == "1"));
        assert!(matches!(tokens[10].token_type, TokenType::Semicolon));
    }
}