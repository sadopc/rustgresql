# RustgreSQL - User Documentation

## Overview

RustgreSQL is a PostgreSQL-like relational database implemented in Rust. It provides comprehensive SQL functionality with ACID compliance through MVCC (Multi-Version Concurrency Control) and WAL (Write-Ahead Logging).

**Version**: 0.1.0
**Status**: Phase 1.1 - Query Execution Engine (Educational/Development Implementation)

## Features

### ✅ Currently Implemented
- **Storage Engine**: B-Tree based storage with buffer pool management and schema evolution
- **Transaction Management**: ACID transactions with MVCC and comprehensive isolation levels
- **Write-Ahead Logging (WAL)**: Durability and crash recovery with DDL transaction support
- **Data Types**: Comprehensive PostgreSQL-compatible data types with type conversion
- **SQL Execution Engine**: Complete SELECT, INSERT, UPDATE, DELETE execution with expression evaluation
- **Query Optimization**: Cost-based optimization with statistics, index selection, and plan caching
- **Advanced SQL Features**: CTEs (Common Table Expressions), Views, Stored Procedures, Window Functions
- **Catalog Management**: System tables for metadata storage with schema and view management
- **Index Management**: B-Tree indexing with primary key and unique constraint support
- **Parallel Execution**: Optional parallel query execution engine
- **File Management**: Persistent database file storage with schema evolution support

### 🚧 In Development
- PostgreSQL wire protocol for external client connections
- Authentication and authorization system
- Advanced query optimization (join ordering, advanced statistics)
- Full SQL standard compliance (advanced features)
- Network interface and connection pooling

## Installation & Setup

### Prerequisites
- Rust 1.70+ (2021 edition)
- Cargo (Rust package manager)

### Building from Source

```bash
# Clone the repository
git clone <repository-url>
cd rustgresql

# Build the project
cargo build

# Build with optimizations (optional)
cargo build --release

# Run tests
cargo test
```

## Quick Start

### 1. Basic Usage

```bash
# Run with default database file (rustgresql.db)
cargo run

# Run with custom database file
cargo run my_database.db
```

### 2. Interactive Mode

When you start RustgreSQL, you'll enter the interactive REPL (Read-Eval-Print Loop):

```
RustgreSQL v0.1.0 - Phase 1.1 Query Execution Engine
Type 'help' for commands, SQL queries, or 'exit' to quit.
rustgresql>
```

### 3. Your First SQL Commands

Try these SQL commands in the interactive prompt:

```sql
-- Create a table
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE,
    age INTEGER
);

-- Insert some data
INSERT INTO users (id, name, email, age) VALUES (1, 'Alice', 'alice@example.com', 30);
INSERT INTO users (id, name, email, age) VALUES (2, 'Bob', 'bob@example.com', 25);

-- Query the data
SELECT * FROM users;

-- Query with conditions
SELECT name, age FROM users WHERE age > 26;

-- Update data
UPDATE users SET age = 31 WHERE name = 'Alice';

-- Delete data
DELETE FROM users WHERE age < 26;
```

### 4. Available Commands

Supported commands:

- `help` - Show available commands and SQL features
- `status` - Display database status information
- `examples` - Show SQL usage examples
- `exit` or `quit` - Exit the database
- **SQL Statements**: Full SQL execution support (SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, etc.)

## Configuration

RustgreSQL uses a configuration system with the following default settings:

```rust
pub struct Config {
    pub page_size: usize,        // 8192 bytes (8KB pages)
    pub buffer_pool_size: usize, // 1000 pages
    pub wal_enabled: bool,       // true
    pub wal_file_path: Option<String>, // "rustgresql.wal"
    pub data_file_path: String,  // "rustgresql.db"
}
```

### Custom Configuration

To use custom settings, modify the `Config` struct in `src/main.rs`:

```rust
let config = Config {
    page_size: 4096,              // 4KB pages
    buffer_pool_size: 500,        // 500 pages in buffer pool
    wal_enabled: true,
    wal_file_path: Some("custom.wal".to_string()),
    data_file_path: "custom.db".to_string(),
};
```

## Architecture

### Core Components

1. **Storage Layer** (`src/storage/`)
    - `BufferPoolManager`: Manages in-memory page caching
    - `FileManager`: Handles disk I/O operations
    - `BTree`: B-Tree implementation for indexing
    - `Page`: Database page structure and management
    - `SchemaEvolutionManager`: Handles schema changes and migrations

2. **Transaction System** (`src/transaction/`)
    - `TransactionManager`: Coordinates transactions
    - `WALManager`: Write-ahead logging for durability
    - `MVCCManager`: Multi-version concurrency control
    - `LockManager`: Row-level locking
    - `DdlTransactionManager`: DDL transaction support
    - `DdlWALManager`: DDL-specific WAL operations

3. **SQL Engine** (`src/sql/`, `src/executor/`)
    - `Parser`: Complete SQL statement parsing (SELECT, INSERT, UPDATE, DELETE, DDL)
    - `Planner`: Query execution planning with optimization
    - `Executor`: Query execution engine with operators
    - `ExpressionEvaluator`: SQL expression evaluation with three-valued logic
    - `QueryRewriter`: Query rewriting and view expansion
    - `ProcedureExecutor`: Stored procedure execution

4. **Query Optimizer** (`src/optimizer/`)
    - `CostModel`: Cost-based query optimization
    - `StatisticsManager`: Table and column statistics
    - `IndexSelector`: Index access path selection
    - `PlanCache`: Query plan caching for performance
    - `RuleEngine`: Optimization rules (pushdown, folding, etc.)

5. **Catalog System** (`src/catalog/`)
    - `CatalogManager`: Unified metadata management
    - `TableManager`: Table definition and management
    - `IndexManager`: Index definition and management
    - `SchemaManager`: Schema management
    - `ViewManager`: View definition and management

6. **Type System** (`src/types/`)
    - `DataType`: PostgreSQL-compatible data types
    - `Value`: Runtime value representation with NULL handling
    - `TypeConverter`: Type conversion and casting utilities

7. **Parallel Execution** (`src/executor/parallel/`) - *Optional Feature*
    - `ParallelExecutor`: Parallel query execution
    - `ParallelExecutorConfig`: Configuration for parallel processing
    - `ResourceManager`: Resource management for parallel tasks

## Supported Data Types

### Numeric Types
- `SMALLINT` / `INT2` - 16-bit integer
- `INTEGER` / `INT` / `INT4` - 32-bit integer
- `BIGINT` / `INT8` - 64-bit integer
- `REAL` / `FLOAT4` - 32-bit floating point
- `DOUBLE PRECISION` / `FLOAT8` - 64-bit floating point
- `NUMERIC(precision, scale)` - Arbitrary precision decimal
- `DECIMAL(precision, scale)` - Arbitrary precision decimal
- `SERIAL` - Auto-incrementing integer
- `BIGSERIAL` - Auto-incrementing bigint

### Character Types
- `CHAR(n)` - Fixed-length character string
- `VARCHAR(n)` - Variable-length character string
- `TEXT` - Variable-length character string (no length limit)

### Binary Types
- `BYTEA` - Binary data

### Date/Time Types
- `DATE` - Calendar date
- `TIME` - Time without time zone
- `TIMESTAMP` - Date and time without time zone
- `INTERVAL` - Time interval

### Boolean Type
- `BOOLEAN` / `BOOL` - Logical true/false values

## Advanced SQL Features

### Common Table Expressions (CTEs)

RustgreSQL supports CTEs for complex query construction:

```sql
-- Basic CTE
WITH user_stats AS (
    SELECT department, COUNT(*) as user_count
    FROM users
    GROUP BY department
)
SELECT * FROM user_stats WHERE user_count > 5;

-- Recursive CTE
WITH RECURSIVE employee_hierarchy AS (
    SELECT id, name, manager_id, 0 as level
    FROM employees
    WHERE manager_id IS NULL

    UNION ALL

    SELECT e.id, e.name, e.manager_id, eh.level + 1
    FROM employees e
    JOIN employee_hierarchy eh ON e.manager_id = eh.id
)
SELECT * FROM employee_hierarchy;
```

### Views

Create virtual tables based on queries:

```sql
-- Create a view
CREATE VIEW active_users AS
SELECT id, name, email
FROM users
WHERE status = 'active';

-- Query the view
SELECT * FROM active_users;

-- Materialized view (refreshed on demand)
CREATE MATERIALIZED VIEW user_summary AS
SELECT department, COUNT(*) as count, AVG(salary) as avg_salary
FROM users
GROUP BY department;

-- Refresh materialized view
REFRESH MATERIALIZED VIEW user_summary;
```

### Window Functions

Advanced analytical functions:

```sql
-- ROW_NUMBER, RANK, DENSE_RANK
SELECT name, department, salary,
       ROW_NUMBER() OVER (ORDER BY salary DESC) as row_num,
       RANK() OVER (ORDER BY salary DESC) as rank,
       DENSE_RANK() OVER (ORDER BY salary DESC) as dense_rank
FROM employees;

-- Running totals and moving averages
SELECT date, sales,
       SUM(sales) OVER (ORDER BY date) as running_total,
       AVG(sales) OVER (ORDER BY date ROWS 2 PRECEDING) as moving_avg
FROM daily_sales;
```

### Stored Procedures and Functions

```sql
-- Create a stored procedure
CREATE PROCEDURE update_user_status(user_id INT, new_status VARCHAR(50))
AS $$
BEGIN
    UPDATE users SET status = new_status, updated_at = CURRENT_TIMESTAMP
    WHERE id = user_id;
END;
$$;

-- Call the procedure
CALL update_user_status(123, 'inactive');

-- Create a function
CREATE FUNCTION calculate_bonus(salary NUMERIC, performance_score INT)
RETURNS NUMERIC
AS $$
BEGIN
    RETURN salary * (performance_score / 100.0);
END;
$$;

-- Use the function
SELECT name, salary, calculate_bonus(salary, performance_score) as bonus
FROM employees;
```

## DDL Statements (Data Definition Language)

RustgreSQL supports comprehensive DDL operations for defining and modifying database schemas.

### CREATE TABLE

Create new tables with columns and constraints:

```sql
-- Basic table creation
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE,
    age INTEGER,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Table with multiple constraints
CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id),
    amount NUMERIC(10, 2) CHECK (amount > 0),
    status VARCHAR(50) DEFAULT 'pending'
);

-- Create table if it doesn't exist
CREATE TABLE IF NOT EXISTS products (
    product_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    price NUMERIC(10, 2) UNIQUE
);
```

**Supported Constraint Types:**
- `PRIMARY KEY` - Uniquely identifies each row
- `UNIQUE` - Ensures column values are unique
- `NOT NULL` - Column must have a value
- `CHECK` - Validates column values against a condition
- `REFERENCES` (Foreign Key) - Links to another table
- `DEFAULT` - Provides default value for column

### DROP TABLE

Remove tables from the database:

```sql
-- Drop a table
DROP TABLE users;

-- Drop table only if it exists (no error if missing)
DROP TABLE IF EXISTS users;
```

### ALTER TABLE

Modify table structure:

```sql
-- Add a new column
ALTER TABLE users ADD COLUMN phone VARCHAR(20);

-- Add a column with default value
ALTER TABLE users ADD COLUMN status VARCHAR(20) DEFAULT 'active';

-- Drop a column
ALTER TABLE users DROP COLUMN phone;

-- Add a constraint
ALTER TABLE users ADD CONSTRAINT uk_email UNIQUE (email);

-- Drop a constraint
ALTER TABLE users DROP CONSTRAINT uk_email;

-- Rename a column
ALTER TABLE users RENAME COLUMN name TO full_name;

-- Rename a table
ALTER TABLE users RENAME TO customer_accounts;
```

### CREATE INDEX / DROP INDEX

Manage indexes for query optimization:

```sql
-- Create an index
CREATE INDEX idx_users_email ON users(email);

-- Create index if it doesn't exist
CREATE INDEX IF NOT EXISTS idx_orders_user_id ON orders(user_id);

-- Drop an index
DROP INDEX idx_users_email;

-- Drop index if it exists
DROP INDEX IF NOT EXISTS idx_users_email;
```

## Constraints

Constraints enforce data integrity rules at the database level. RustgreSQL supports both column-level and table-level constraints.

### Column-Level Constraints

Applied to individual columns during column definition:

```sql
CREATE TABLE employees (
    -- NOT NULL constraint
    employee_id INTEGER NOT NULL,

    -- UNIQUE constraint
    email VARCHAR(255) UNIQUE,

    -- PRIMARY KEY constraint
    id INTEGER PRIMARY KEY,

    -- DEFAULT constraint
    hire_date DATE DEFAULT CURRENT_DATE,

    -- CHECK constraint
    salary NUMERIC(10, 2) CHECK (salary > 0),

    -- FOREIGN KEY constraint
    department_id INTEGER REFERENCES departments(id)
);
```

### Table-Level Constraints

Applied to one or more columns after all columns are defined:

```sql
CREATE TABLE order_items (
    order_id INTEGER,
    product_id INTEGER,
    quantity INTEGER,
    price NUMERIC(10, 2),

    -- Composite PRIMARY KEY
    PRIMARY KEY (order_id, product_id),

    -- FOREIGN KEY with named constraint
    CONSTRAINT fk_orders FOREIGN KEY (order_id) REFERENCES orders(id),

    -- UNIQUE constraint on multiple columns
    CONSTRAINT uk_product_order UNIQUE (order_id, product_id),

    -- CHECK constraint with complex condition
    CHECK (quantity > 0 AND price > 0)
);
```

### Constraint Types Reference

| Constraint | Level | Purpose | Example |
|-----------|-------|---------|---------|
| PRIMARY KEY | Both | Uniquely identifies rows, enforces NOT NULL | `id INTEGER PRIMARY KEY` |
| UNIQUE | Both | Ensures all values are unique | `email VARCHAR(255) UNIQUE` |
| NOT NULL | Column | Prevents NULL values | `name VARCHAR(255) NOT NULL` |
| DEFAULT | Column | Provides default value | `status VARCHAR(20) DEFAULT 'active'` |
| CHECK | Both | Validates values against expression | `CHECK (age >= 18)` |
| FOREIGN KEY | Table | References another table for referential integrity | `FOREIGN KEY (user_id) REFERENCES users(id)` |

### Constraint Validation

Constraints are validated at multiple points:

1. **Parse-time**: Syntax validation during SQL parsing
2. **Create-time**: Semantic validation when creating tables
3. **Runtime**: Enforcement during INSERT/UPDATE operations

### Foreign Key Referential Integrity

When a FOREIGN KEY constraint is defined, RustgreSQL ensures:

- Insert/Update values must exist in the referenced table
- CASCADE behavior removes dependent rows when referenced row is deleted (when specified)
- RESTRICT behavior prevents deletion of referenced rows (default)

```sql
-- Example: E-commerce schema with referential integrity
CREATE TABLE customers (
    customer_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL
);

CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER NOT NULL REFERENCES customers(customer_id),
    order_date DATE DEFAULT CURRENT_DATE
);
```

## Transaction Management

### Isolation Levels

RustgreSQL supports PostgreSQL-style isolation levels:

1. `ReadUncommitted` - Lowest isolation (not recommended)
2. `ReadCommitted` - Default, prevents dirty reads
3. `RepeatableRead` - Prevents non-repeatable reads
4. `Serializable` - Full isolation, serializable execution

### Transaction Commands (Planned)

```sql
BEGIN TRANSACTION;
-- Your SQL statements here
COMMIT;

-- Or rollback
ROLLBACK;
```

### ACID Properties

- **Atomicity**: Transactions are all-or-nothing
- **Consistency**: Database remains in valid state
- **Isolation**: Concurrent transactions don't interfere
- **Durability**: Committed transactions survive crashes

## Storage Architecture

### Page Structure

- **Page Size**: Configurable (default 8KB)
- **Page Types**: Data, Index, System, Free
- **Buffer Pool**: In-memory page caching

### File Organization

- `rustgresql.db` - Main database file
- `rustgresql.wal` - Write-ahead log file (if enabled)

### B-Tree Implementation

- Primary data structure for indexing
- Supports range queries and ordering
- Integrated with buffer pool manager

## Development Status

### Current Capabilities

✅ **Phase 1.1 - Query Execution Engine COMPLETED**
- Full SQL statement execution (SELECT, INSERT, UPDATE, DELETE)
- Comprehensive expression evaluation with three-valued logic
- NULL value handling and type casting
- DDL operations (CREATE/DROP TABLE, INDEX, VIEW)
- CTEs (Common Table Expressions) and subqueries
- Window functions and aggregate operations
- Stored procedures and functions
- Cost-based query optimization
- Index selection and plan caching
- Parallel query execution (optional feature)

### Current Limitations

1. **Network Interface**: No PostgreSQL wire protocol yet (CLI-only access)
2. **Authentication**: No user management or authorization system
3. **Advanced Optimization**: Some optimization rules still developing
4. **Production Features**: Monitoring, backup/restore utilities pending

### Roadmap

#### ✅ Phase 1: Core SQL Execution - **COMPLETED**
- [x] INSERT, UPDATE, DELETE statement execution
- [x] SELECT with WHERE, JOIN, GROUP BY, HAVING
- [x] Expression evaluation and type conversion
- [x] DDL operations (tables, indexes, views)
- [x] CTEs and subqueries
- [x] Window functions and aggregates

#### 🚧 Phase 2: Advanced SQL Features - **IN PROGRESS**
- [x] Views and materialized views
- [x] Stored procedures and functions
- [ ] Advanced JOIN types and optimization
- [ ] Full subquery support in all contexts
- [ ] Advanced window functions

#### 📋 Phase 3: Performance & Scalability
- [x] Query optimization framework
- [x] Index management and selection
- [x] Parallel query execution
- [ ] Advanced statistics and cost modeling
- [ ] Connection pooling
- [ ] Memory management optimization

#### Phase 4: Production Features
- [ ] Network protocol implementation
- [ ] Authentication and authorization
- [ ] Backup and restore utilities
- [ ] Monitoring and logging

## API Reference (Library Usage)

### Basic Database Operations

```rust
use rustgresql::{Database, Config, sql::parse_sql, executor::ExecutionEngine};

fn main() -> rustgresql::Result<()> {
    // Create configuration
    let config = Config::default();

    // Create or open database
    let db = Database::new(config)?;
    db.initialize()?;

    // Initialize execution engine
    let execution_engine = ExecutionEngine::new();

    // Execute SQL directly
    let statements = parse_sql("SELECT * FROM users WHERE age > 21;")?;
    for statement in statements {
        let (result, stats) = execution_engine.execute_query(&statement)?;
        println!("Query executed in {}ms", stats.execution_time_ms);
    }

    Ok(())
}
```

### Advanced Features

```rust
use rustgresql::optimizer::OptimizedQueryPlanner;

// Use the query optimizer
let planner = OptimizedQueryPlanner::new();
let optimized_plan = planner.optimize_query(&parsed_query)?;

// Parallel execution (if feature enabled)
#[cfg(feature = "parallel")]
{
    use rustgresql::executor::parallel::ParallelExecutor;
    let parallel_executor = ParallelExecutor::new(config);
    let result = parallel_executor.execute_parallel(plan)?;
}
```

### Configuration Options

```rust
use rustgresql::Config;

let config = Config {
    page_size: 8192,           // Page size in bytes
    buffer_pool_size: 1000,     // Number of pages to cache
    wal_enabled: true,          // Enable write-ahead logging
    wal_file_path: Some("db.wal".to_string()),
    data_file_path: "db.dat".to_string(),
};
```

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test module
cargo test storage

# Run with output
cargo test -- --nocapture
```

### Benchmarking

```bash
# Run benchmarks
cargo bench

# Run specific benchmark
cargo bench btree_bench
```

## Contributing

### Development Setup

1. Install Rust toolchain
2. Clone repository
3. Run `cargo build` to verify setup
4. Run `cargo test` to run tests

### Code Style

- Use `cargo fmt` for formatting
- Use `cargo clippy` for linting
- Follow Rust naming conventions
- Add unit tests for new features

### Submitting Changes

1. Fork the repository
2. Create feature branch
3. Make changes with tests
4. Submit pull request

## Troubleshooting

### Common Issues

1. **Build Failures**: Ensure Rust 1.70+ is installed
2. **Runtime Errors**: Check file permissions for database files
3. **Performance**: Adjust buffer pool size for your system

### Debug Mode

```bash
# Enable debug logging
RUST_LOG=debug cargo run

# Run with debug assertions
cargo run
```

## License

MIT License - see LICENSE file for details.

## Credits

Educational implementation inspired by:
- PostgreSQL architecture and design
- "Database System Concepts" by Silberschatz, Korth, Sudarshan
- "Designing Data-Intensive Applications" by Martin Kleppmann

---

**Note**: This is an educational implementation for learning database internals. It is not intended for production use.