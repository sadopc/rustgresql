# RustgreSQL Documentation

## Overview

**RustgreSQL** is an educational relational database system implemented in Rust that aims to be compatible with PostgreSQL's SQL dialect. It provides ACID transactions, sophisticated query execution, and a comprehensive storage engine - all written from scratch to demystify database internals.

## 🚀 Quick Start

```bash
# Build and run the database
cargo run

# Or specify a custom database file
cargo run -- mydatabase.db
```

This starts an interactive REPL where you can execute SQL commands:

```sql
-- Create a table
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    age INTEGER CHECK (age >= 0)
);

-- Insert data
INSERT INTO users VALUES 
    (1, 'Alice', 'alice@example.com', 25),
    (2, 'Bob', 'bob@example.com', 30);

-- Query data
SELECT name, age FROM users WHERE age >= 25 ORDER BY name;
```

## 🏗️ Architecture

RustgreSQL follows a layered architecture with clear separation of concerns:

```
┌─────────────────────────────────────────┐
│              REPL Interface             │
│         (main.rs + rustyline)           │
├─────────────────────────────────────────┤
│         SQL Parser & AST                │
│        (sql/parser.rs + lexer.rs)       │
├─────────────────────────────────────────┤
│        Query Planner & Optimizer        │
│      (executor/planner.rs, optimizer/)  │
├─────────────────────────────────────────┤
│         Execution Engine                │
│       (executor/engine.rs, operators/)  │
├─────────────────────────────────────────┤
│      Transaction Manager (MVCC)         │
│     (transaction/manager.rs, mvcc.rs)   │
├─────────────────────────────────────────┤
│         Storage Engine                  │
│   (storage/btree.rs, buffer.rs, page.rs)│
└─────────────────────────────────────────┘
```

## 📁 Module Structure

### Core Modules

#### 1. **SQL Parsing** (`src/sql/`)
- **Purpose**: Converts SQL text into an Abstract Syntax Tree (AST)
- **Components**:
  - `lexer.rs` - Tokenizes SQL input
  - `parser.rs` - Hand-written recursive descent parser
  - `ast.rs` - AST node definitions
- **Features**:
  - Full SQL grammar support including CTEs and window functions
  - Error reporting with line/column numbers
  - No external parser generator dependencies

#### 2. **Query Execution** (`src/executor/`)
- **Purpose**: Executes queries against the database
- **Components**:
  - `engine.rs` - Main execution engine
  - `operators.rs` - Physical query operators (scan, join, aggregate, etc.)
  - `planner.rs` - Converts AST to execution plans
  - `expression.rs` - Expression evaluation engine
- **Features**:
  - Iterator-based execution model
  - Support for complex joins (nested loop, hash join, merge join)
  - Aggregate functions (COUNT, SUM, AVG, MIN, MAX)
  - Window functions (ROW_NUMBER, RANK, LAG, LEAD)
  - Common Table Expressions (CTEs)

#### 3. **Storage Engine** (`src/storage/`)
- **Purpose**: Manages persistent data storage and indexing
- **Components**:
  - `btree.rs` - B+ tree implementation for indexing
  - `buffer.rs` - Buffer pool manager with LRU eviction
  - `page.rs` - 8KB page format and management
  - `file_manager.rs` - Low-level file I/O operations
- **Features**:
  - Page-based storage (8KB pages)
  - B+ tree indexes for O(log n) lookups
  - Buffer pool with configurable size
  - Page-level locking and versioning

#### 4. **Transaction Management** (`src/transaction/`)
- **Purpose**: Provides ACID transactions with MVCC
- **Components**:
  - `manager.rs` - Transaction lifecycle management
  - `mvcc.rs` - Multi-Version Concurrency Control
  - `wal.rs` - Write-Ahead Logging for crash recovery
  - `lock.rs` - Lock manager for concurrency control
- **Features**:
  - Full ACID compliance
  - Serializable isolation level
  - Crash recovery via WAL
  - Deadlock detection and prevention

#### 5. **System Catalog** (`src/catalog/`)
- **Purpose**: Manages database metadata and schema
- **Components**:
  - `table.rs` - Table definitions and system tables
  - `schema.rs` - Database schema management
  - `index.rs` - Index metadata management
  - `view.rs` - View definitions and dependencies
- **Features**:
  - Self-describing database schema
  - Constraint enforcement (PRIMARY KEY, FOREIGN KEY, UNIQUE, CHECK)
  - Schema evolution support

#### 6. **Type System** (`src/types/`)
- **Purpose**: Data type definitions and conversions
- **Components**:
  - `data_type.rs` - Core data type definitions
  - `value.rs` - Value representations and operations
  - `convert.rs` - Type conversion utilities
- **Supported Types**:
  - INTEGER, BIGINT, REAL (numeric)
  - TEXT (variable-length strings)
  - BOOLEAN (three-valued logic)
  - TIMESTAMP (date/time)
  - NULL for missing values

#### 7. **Query Optimization** (`src/optimizer/`)
- **Purpose**: Optimizes query execution plans
- **Components**:
  - `query_optimizer.rs` - Main optimization logic
  - `cost_model.rs` - Cost estimation for different plans
  - `index_selection.rs` - Index usage optimization
  - `join_ordering.rs` - Join sequence optimization
- **Features**:
  - Cost-based query optimization
  - Index selection algorithms
  - Join ordering optimization

#### 8. **Parallel Execution** (`src/executor/parallel/`)
- **Purpose**: Enables parallel query execution
- **Components**:
  - `executor.rs` - Parallel execution coordinator
  - `scheduler.rs` - Task scheduling and workload distribution
  - `resource_manager.rs` - Resource allocation and monitoring
- **Features**:
  - Multi-threaded query execution
  - Dynamic load balancing
  - Resource monitoring and management

## 🔧 Key Components

### Database Instance (`src/lib.rs`)
The main database entry point that coordinates all subsystems:

```rust
pub struct Database {
    pub config: Config,
    buffer_manager: Arc<BufferPoolManager>,
    catalog_manager: Arc<CatalogManager>,
    pub transaction_manager: Arc<TransactionManager>,
}
```

### Query Processing Pipeline
1. **Parse**: SQL → AST (Abstract Syntax Tree)
2. **Plan**: AST → Physical Execution Plan
3. **Execute**: Plan → Result Set
4. **Optimize**: Alternative plan generation and selection

### Storage Model
- **Page Size**: 8KB fixed page size
- **Index Structure**: B+ tree with variable-length keys
- **Buffer Pool**: LRU eviction policy with configurable size
- **File Format**: Custom binary format with page checksums

### Transaction Model
- **Isolation**: Serializable using MVCC
- **Locking**: Page-level and row-level locks
- **Recovery**: Write-Ahead Logging (WAL)
- **Durability**: fsync() after commit

## 📊 Supported SQL Features

### Data Definition Language (DDL)
```sql
CREATE TABLE table_name (
    column_name TYPE [constraints],
    ...
);

CREATE INDEX index_name ON table_name (column_name);

CREATE VIEW view_name AS SELECT ...;

ALTER TABLE table_name ADD COLUMN new_column TYPE;
```

### Data Manipulation Language (DML)
```sql
SELECT column1, column2 FROM table WHERE condition;
INSERT INTO table VALUES (...);
UPDATE table SET column = value WHERE condition;
DELETE FROM table WHERE condition;
```

### Advanced Features
- **Joins**: INNER, LEFT, RIGHT, FULL OUTER
- **Aggregates**: COUNT, SUM, AVG, MIN, MAX
- **Window Functions**: ROW_NUMBER, RANK, LAG, LEAD
- **CTEs**: WITH clause for subquery factoring
- **Subqueries**: Correlated and non-correlated
- **Expressions**: Arithmetic, string functions, case expressions

## 🛡️ Reliability Features

### ACID Compliance
- **Atomicity**: All changes in a transaction or none
- **Consistency**: Database constraints maintained
- **Isolation**: Serializable transactions
- **Durability**: Changes survive crashes

### Crash Recovery
- Write-Ahead Logging (WAL) ensures durability
- Automatic recovery on database startup
- Page-level checksums for corruption detection
- Transaction replay for incomplete commits

### Concurrency Control
- Multi-Version Concurrency Control (MVCC)
- Snapshot isolation for readers
- Conflict detection for concurrent writes
- Deadlock detection and resolution

## 🔬 Testing

The codebase includes comprehensive tests:

```bash
# Run all tests
cargo test

# Run specific test categories
cargo test catalog::          # Catalog tests
cargo test executor::         # Execution engine tests
cargo test storage::          # Storage engine tests

# Run with output
cargo test -- --nocapture
```

### Test Categories
- **Unit Tests**: Individual component testing
- **Integration Tests**: End-to-end functionality
- **Performance Tests**: Benchmarking core operations
- **Recovery Tests**: Crash recovery validation

## ⚡ Performance Characteristics

### Storage Performance
- **B-Tree Lookup**: O(log n) for indexed searches
- **Sequential Scan**: O(n) with page-level optimization
- **Index Range Scan**: O(log n + k) where k is result size
- **Buffer Hit Ratio**: Configurable LRU buffer pool

### Query Performance
- **Simple Queries**: Sub-millisecond execution
- **Complex Joins**: Optimized join algorithms
- **Aggregation**: Hash-based and sort-based strategies
- **Parallel Execution**: Multi-core utilization

## 🚧 Development Status

### Completed Features ✅
- [x] Full SQL parser with error handling
- [x] Complete DDL and DML support
- [x] B-tree storage engine with indexing
- [x] Buffer pool management
- [x] ACID transactions with MVCC
- [x] Write-Ahead Logging (WAL)
- [x] System catalog and metadata management
- [x] Complex query execution (joins, aggregates, window functions)
- [x] Query optimization framework
- [x] Parallel execution support

### In Development 🚧
- [ ] Advanced query optimization rules
- [ ] Distributed query execution
- [ ] Advanced indexing strategies (bitmap, GiST)
- [ ] Query result caching

### Roadmap 🗺️
- [ ] Read replica support
- [ ] Foreign data wrappers
- [ ] Advanced analytics functions
- [ ] Database connection protocols (TCP/IP, HTTP)

## 🏗️ Building and Contributing

### Prerequisites
- Rust 1.70+ 
- Cargo (comes with Rust)

### Build Commands
```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run with specific features
cargo run --features parallel
```

### Code Organization
- Follow existing code style and patterns
- Add tests for new functionality
- Update documentation for API changes
- Use meaningful commit messages

### Development Guidelines
1. **Modularity**: Keep components loosely coupled
2. **Error Handling**: Use the `Result` type consistently
3. **Testing**: Aim for high test coverage
4. **Documentation**: Document public APIs
5. **Performance**: Profile before optimizing

## 📚 Educational Value

This codebase serves as a learning resource for understanding:

- **Database Internals**: How database systems work under the hood
- **Rust Programming**: Advanced Rust patterns for systems programming
- **Algorithm Implementation**: B-trees, hash tables, sorting algorithms
- **Concurrency Control**: MVCC, locking, and deadlock detection
- **Query Processing**: Parsing, planning, and execution of SQL queries
- **Storage Management**: Page-based storage and buffer management
- **Crash Recovery**: WAL-based recovery mechanisms

## ⚠️ Important Notes

**This is an educational project and should NOT be used in production environments.** While it implements most database features correctly, it lacks:

- Production-grade security
- Performance optimization for large datasets
- Network protocols for client access
- Comprehensive backup and restore
- Monitoring and administration tools
- Production deployment documentation

## 📖 Further Reading

- Database System Concepts (Silberschatz, Galvin, Gagne)
- Transaction Processing (Gray, Reuter)
- Database Management Systems (Ramakrishnan, Gehrke)
- The Rust Book (for Rust-specific implementation details)
- PostgreSQL documentation for SQL compatibility reference

---

**RustgreSQL** - Because every developer should understand what happens when they type `SELECT * FROM users;`.