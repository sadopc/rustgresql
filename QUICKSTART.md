# RustgreSQL Quick Start Guide

## Getting Started in 5 Minutes

### 1. Run Your First Database

```bash
# Start the database with default settings
cargo run
```

You should see:
```
RustgreSQL v0.1.0
Type 'help' for commands or 'exit' to quit.
rustgresql>
```

### 2. Basic Commands

```bash
rustgresql> help
Available commands:
  help     - Show this help message
  status   - Show database status
  exit     - Exit the program

rustgresql> status
Database is running
Data file: rustgresql.db

rustgresql> exit
Goodbye!
```

### 3. Using a Custom Database File

```bash
# Create and use a custom database file
cargo run my_custom_database.db
```

### 4. Understanding the Current State

**What Works Right Now:**
- ✅ Database initialization and file creation
- ✅ Basic interactive commands (help, status, exit)
- ✅ Transaction management system
- ✅ Storage engine with B-Tree indexing
- ✅ Write-ahead logging (WAL)
- ✅ PostgreSQL-compatible data types

**What's Coming Next:**
- 🚧 SQL statement execution (INSERT, SELECT, UPDATE, DELETE)
- 🚧 Table creation and management
- 🚧 Query optimization
- 🚧 Network protocol for external clients

## Architecture Overview

```
┌─────────────────────────────────────┐
│           RustgreSQL v0.1.0          │
├─────────────────────────────────────┤
│  Interactive REPL (Read-Eval-Print)  │
├─────────────────────────────────────┤
│         SQL Parser (Basic)           │
├─────────────────────────────────────┤
│       Transaction Manager            │
│  ┌─────────────┬─────────────────┐   │
│  │   WAL       │      MVCC       │   │
│  │ (Durability)│ (Concurrency)   │   │
│  └─────────────┴─────────────────┘   │
├─────────────────────────────────────┤
│         Storage Engine               │
│  ┌─────────────┬─────────────────┐   │
│  │Buffer Pool  │   B-Tree Index  │   │
│  │(Memory)     │   (Data)        │   │
│  └─────────────┴─────────────────┘   │
├─────────────────────────────────────┤
│         File System                  │
│  rustgresql.db    rustgresql.wal     │
└─────────────────────────────────────┘
```

## File Structure

When you run RustgreSQL, it creates files in your current directory:

```
rustgresql/               # Project directory
├── src/                  # Source code
├── target/               # Compiled binary
├── rustgresql.db         # Main database file (created when run)
├── rustgresql.wal        # Write-ahead log file (created if WAL enabled)
└── Cargo.toml           # Project configuration
```

## Configuration

Default settings work for most cases, but you can customize:

```rust
// In src/main.rs
let config = Config {
    page_size: 8192,              // 8KB pages (default)
    buffer_pool_size: 1000,        // Cache 1000 pages in memory
    wal_enabled: true,             // Enable durability
    wal_file_path: Some("db.wal".to_string()),
    data_file_path: "mydb.db".to_string(),
};
```

## Development Commands

```bash
# Build the project
cargo build

# Run with debug output
RUST_LOG=debug cargo run

# Run tests
cargo test

# Format code
cargo fmt

# Check for issues
cargo clippy
```

## What's Happening Under the Hood

### When You Start RustgreSQL:

1. **Database Initialization**: Creates database files if they don't exist
2. **Buffer Pool Setup**: Allocates memory for caching database pages
3. **WAL Initialization**: Sets up write-ahead logging for durability
4. **Catalog Loading**: Loads system tables and metadata
5. **REPL Start**: Begins interactive command prompt

### Supported Data Types (Ready for Future SQL):

- **Numbers**: SMALLINT, INTEGER, BIGINT, REAL, DOUBLE PRECISION
- **Strings**: CHAR(n), VARCHAR(n), TEXT
- **Binary**: BYTEA
- **Dates**: DATE, TIME, TIMESTAMP, INTERVAL
- **Boolean**: BOOLEAN, BOOL
- **Special**: SERIAL, BIGSERIAL (auto-incrementing)

### Transaction Isolation Levels:

```rust
use rustgresql::transaction::IsolationLevel;

// Available levels:
IsolationLevel::ReadUncommitted    // Lowest isolation
IsolationLevel::ReadCommitted      // Default, prevents dirty reads
IsolationLevel::RepeatableRead     // Prevents non-repeatable reads
IsolationLevel::Serializable       // Full isolation
```

## Next Steps

1. **Explore the Code**: Look at `src/lib.rs` to understand the main components
2. **Run Tests**: `cargo test` to see the testing framework
3. **Check the Architecture**: Review module documentation in `src/`
4. **Watch for Updates**: SQL execution is the next major feature

## Getting Help

- **Documentation**: See `README.md` for detailed information
- **Code Comments**: Each module has extensive documentation
- **Issues**: Report bugs or request features on the project repository

---

**Note**: RustgreSQL v0.1.0 is an educational implementation focused on teaching database internals. Future versions will add full SQL capabilities.