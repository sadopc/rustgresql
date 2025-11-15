# RustgreSQL - User Documentation

## Overview

RustgreSQL is a PostgreSQL-like relational database implemented in Rust. It provides basic SQL functionality with ACID compliance through MVCC (Multi-Version Concurrency Control) and WAL (Write-Ahead Logging).

**Version**: 0.1.0
**Status**: Educational/Development Implementation

## Features

### ✅ Currently Implemented
- **Storage Engine**: B-Tree based storage with buffer pool management
- **Transaction Management**: ACID transactions with MVCC
- **Write-Ahead Logging (WAL)**: Durability and crash recovery
- **Data Types**: Comprehensive PostgreSQL-compatible data types
- **SQL Parsing**: Basic SQL statement parsing
- **Catalog Management**: System tables for metadata storage
- **File Management**: Persistent database file storage

### 🚧 In Development
- SQL execution engine
- Query optimization
- Index management
- Full SQL standard compliance

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
RustgreSQL v0.1.0
Type 'help' for commands or 'exit' to quit.
rustgresql>
```

### 3. Available Commands

Currently supported commands:

- `help` - Show available commands
- `status` - Display database status information
- `exit` or `quit` - Exit the database

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

2. **Transaction System** (`src/transaction/`)
   - `TransactionManager`: Coordinates transactions
   - `WALManager`: Write-ahead logging for durability
   - `MVCCManager`: Multi-version concurrency control
   - `LockManager`: Row-level locking

3. **SQL Engine** (`src/sql/`, `src/executor/`)
   - `Parser`: SQL statement parsing
   - `Planner`: Query execution planning
   - `Executor`: Query execution engine

4. **Catalog System** (`src/catalog/`)
   - `CatalogManager`: Metadata management
   - `TableManager`: Table definition and management
   - `IndexManager`: Index definition and management

5. **Type System** (`src/types/`)
   - `DataType`: PostgreSQL-compatible data types
   - `Value`: Runtime value representation
   - `TypeConverter`: Type conversion utilities

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

### Current Limitations

1. **SQL Execution**: Basic parsing only, no execution yet
2. **Query Optimization**: No optimization phase
3. **Indexes**: B-Tree structure exists but not fully integrated
4. **SQL Standards**: Limited SQL compliance
5. **Network Interface**: No client/server protocol yet
6. **Authentication**: No user management system

### Roadmap

#### Phase 1: Core SQL Execution
- [ ] INSERT statement execution
- [ ] SELECT statement execution
- [ ] UPDATE statement execution
- [ ] DELETE statement execution
- [ ] Basic WHERE clause support

#### Phase 2: Advanced SQL Features
- [ ] JOIN operations
- [ ] Aggregate functions
- [ ] GROUP BY and HAVING
- [ ] Subqueries
- [ ] Views

#### Phase 3: Performance & Scalability
- [ ] Query optimization
- [ ] Index management
- [ ] Parallel query execution
- [ ] Connection pooling

#### Phase 4: Production Features
- [ ] Network protocol implementation
- [ ] Authentication and authorization
- [ ] Backup and restore utilities
- [ ] Monitoring and logging

## API Reference (Library Usage)

### Basic Database Operations

```rust
use rustgresql::{Database, Config};

fn main() -> rustgresql::Result<()> {
    // Create configuration
    let config = Config::default();

    // Create or open database
    let db = Database::new(config)?;
    db.initialize()?;

    // Begin transaction
    let tx = db.begin_transaction()?;

    // Use transaction...

    Ok(())
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
# Run benchmarks (when implemented)
cargo bench
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