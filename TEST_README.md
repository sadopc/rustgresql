# RustgreSQL Test Suite

This directory contains comprehensive test files for the RustgreSQL database.

## Test Files

### 1. `test_basic.sql` - Basic Operations
Tests fundamental database operations:
- CREATE TABLE
- INSERT
- SELECT (with various WHERE clauses)
- UPDATE
- DELETE
- COUNT aggregate

**How to run:**
```bash
cargo run test.db < test_basic.sql
```

Or run interactively:
```bash
cargo run test.db
```
Then paste commands one by one.

---

### 2. `test_advanced.sql` - Advanced Operations
Tests more complex scenarios:
- Multiple tables (products, orders)
- Complex WHERE conditions (AND, OR)
- Float data types
- Batch operations
- Multiple updates and deletes

**How to run:**
```bash
cargo run test.db < test_advanced.sql
```

---

### 3. `test_data_types.sql` - Data Type Testing
Tests various data types:
- INTEGER
- VARCHAR
- FLOAT
- BOOLEAN
- TEXT
- NULL handling

**How to run:**
```bash
cargo run test.db < test_data_types.sql
```

---

### 4. `test_indexes.sql` - Index Operations
Tests index functionality:
- CREATE INDEX
- Querying with indexes
- DROP INDEX
- Multiple indexes on one table

**How to run:**
```bash
cargo run test.db < test_indexes.sql
```

---

## Quick Start

### Interactive Mode
Start the database in interactive mode:
```bash
cargo run test.db
```

Then type SQL commands at the prompt:
```sql
rustgresql> CREATE TABLE users (id INTEGER, name VARCHAR(100));
rustgresql> INSERT INTO users VALUES (1, 'Alice');
rustgresql> SELECT * FROM users;
rustgresql> .quit
```

### Batch Mode
Run an entire test file:
```bash
cargo run test.db < test_basic.sql
```

### Special Commands
- `.help` - Show help information
- `.quit` - Exit the database
- `.exit` - Exit the database

---

## Test Scenarios Covered

### ✅ Basic CRUD Operations
- [x] CREATE TABLE
- [x] INSERT
- [x] SELECT
- [x] UPDATE
- [x] DELETE

### ✅ Query Features
- [x] WHERE clauses
- [x] Comparison operators (=, >, <, >=, <=)
- [x] Boolean operations (AND, OR)
- [x] COUNT aggregation

### ✅ Data Types
- [x] INTEGER
- [x] VARCHAR
- [x] FLOAT
- [x] BOOLEAN
- [x] TEXT

### ✅ Index Operations
- [x] CREATE INDEX
- [x] DROP INDEX
- [x] Index-based queries

### ✅ Complex Scenarios
- [x] Multiple tables
- [x] Multiple conditions
- [x] Batch inserts
- [x] Batch updates

---

## Expected Results

### Successful CREATE TABLE
```
Table 'employees' created successfully
```

### Successful INSERT
```
1 row inserted
```

### SELECT Results
```
+----+------------+--------------+--------+------------+
| id | name       | department   | salary | is_manager |
+----+------------+--------------+--------+------------+
| 1  | John Doe   | Engineering  | 75000  | false      |
| 2  | Jane Smith | Engineering  | 85000  | true       |
+----+------------+--------------+--------+------------+
2 rows
```

### Successful UPDATE
```
1 row updated
```

### Successful DELETE
```
1 row deleted
```

---

## Troubleshooting

### If you get errors:
1. Make sure you're in the rustgresql directory
2. Build the project first: `cargo build`
3. Start fresh with a new database file: `cargo run fresh.db`
4. Check syntax - RustgreSQL uses PostgreSQL-like syntax

### Common Issues:
- **"Table already exists"** - The table name is taken. Use a different name or restart with a new database file.
- **"Column not found"** - Check your column names match what you created.
- **"Type mismatch"** - Ensure your INSERT values match the column types.

---

## Clean Up

To start fresh:
```bash
# Delete database files
rm test.db test.db.wal

# Start with a clean database
cargo run fresh.db
```

---

## Notes

- This is an educational database system
- Not recommended for production use
- Data is persisted to disk (`.db` file)
- Write-ahead log is stored in `.wal` file
- ACID compliance is implemented
- Transactions are supported
