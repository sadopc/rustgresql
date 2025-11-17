# Quick Test Commands for RustgreSQL

Copy and paste these commands to quickly test the database.

## Start the Database

```bash
cargo run test.db
```

## Quick Copy-Paste Tests

### Test 1: Create and Insert
```sql
CREATE TABLE users (id INTEGER, name VARCHAR(100), age INTEGER);
INSERT INTO users VALUES (1, 'Alice', 25);
INSERT INTO users VALUES (2, 'Bob', 30);
INSERT INTO users VALUES (3, 'Charlie', 35);
SELECT * FROM users;
```

### Test 2: Filtering
```sql
SELECT name, age FROM users WHERE age > 25;
SELECT * FROM users WHERE name = 'Bob';
```

### Test 3: Update
```sql
UPDATE users SET age = 26 WHERE id = 1;
SELECT * FROM users WHERE id = 1;
```

### Test 4: Count
```sql
SELECT COUNT(*) FROM users;
SELECT COUNT(*) FROM users WHERE age > 30;
```

### Test 5: Delete
```sql
DELETE FROM users WHERE id = 3;
SELECT * FROM users;
```

### Test 6: Another Table
```sql
CREATE TABLE products (product_id INTEGER, name VARCHAR(100), price FLOAT);
INSERT INTO products VALUES (1, 'Laptop', 999.99);
INSERT INTO products VALUES (2, 'Mouse', 29.99);
INSERT INTO products VALUES (3, 'Keyboard', 79.99);
SELECT * FROM products;
SELECT name, price FROM products WHERE price < 100;
```

### Test 7: Index
```sql
CREATE INDEX idx_user_id ON users (id);
SELECT * FROM users WHERE id = 2;
DROP INDEX idx_user_id;
```

### Test 8: Booleans
```sql
CREATE TABLE tasks (id INTEGER, description VARCHAR(100), completed BOOLEAN);
INSERT INTO tasks VALUES (1, 'Write tests', true);
INSERT INTO tasks VALUES (2, 'Review code', false);
INSERT INTO tasks VALUES (3, 'Deploy app', false);
SELECT * FROM tasks WHERE completed = false;
```

## Exit
```sql
.quit
```

## Run All Basic Tests
```bash
# In another terminal
cargo run test.db < test_basic.sql
```

## Run Individual Test Files
```bash
# Basic operations
cargo run test.db < test_basic.sql

# Advanced operations
cargo run test.db < test_advanced.sql

# Data types
cargo run test.db < test_data_types.sql

# Indexes
cargo run test.db < test_indexes.sql
```
