# How to Create and Use RustgreSQL Database Files

## 🎯 Quick Summary

**Creating a `.db` file is automatic!** When you run RustgreSQL with a database file name, it automatically creates the file if it doesn't exist.

## 📝 Step-by-Step Guide

### 1. Create a New Database File

```bash
# Method 1: Use default filename (rustgresql.db)
cargo run

# Method 2: Specify custom filename
cargo run my_database.db

# Method 3: Use full path
cargo run /path/to/my_database.db
```

**What happens:**
- If the file doesn't exist → Creates a new database file
- If the file exists → Opens the existing database file

### 2. Verify Database File Creation

```bash
# Check if database files were created
ls -la *.db *.wal

# Example output:
# -rw-r--r-- 1 user user 8192 Nov 14 11:18 my_database.db
# -rw-r--r-- 1 user user  4096 Nov 14 11:18 my_database.wal
```

**Files created:**
- `.db` file - Main database file with data and indexes
- `.wal` file - Write-ahead log for crash recovery (if WAL enabled)

### 3. Interact with the Database

```bash
# Start the interactive REPL
cargo run my_database.db
```

**Available commands:**
```bash
rustgresql> help          # Show available commands
rustgresql> status        # Show database status
rustgresql> exit          # Exit the database
```

### 4. Database File Information

```bash
# Check file size
du -h my_database.db

# Check file details
stat my_database.db

# View file contents (binary - for advanced users)
hexdump -C my_database.db | head -10
```

## 🔧 Advanced Configuration

### Custom Database Settings

You can modify the configuration in `src/main.rs`:

```rust
let config = Config {
    page_size: 4096,              // 4KB pages (default: 8192)
    buffer_pool_size: 500,        // 500 pages in memory (default: 1000)
    wal_enabled: true,             // Enable crash recovery
    wal_file_path: Some("custom.wal".to_string()),
    data_file_path: "custom.db".to_string(),
};
```

### Database File Structure

```
my_database.db  ← Main database file
├── Header      → Database metadata and schema
├── Data Pages  → Actual table data
├── Index Pages → B-Tree indexes
└── Free Pages  → Available space for allocation

my_database.wal ← Write-ahead log (if enabled)
├── Transaction records
├── Change logs
└── Checkpoint data
```

## 📊 What's Inside the Database File?

### File Header
- Magic number: `0x5253514C44544142` ("RUSTGDB")
- Version information
- Page size
- Total number of pages
- Free page list

### Page Structure (8KB default)
```
Page Header (24 bytes)
├── Page ID
├── Page Type (Data/Index/Free)
├── LSN (Log Sequence Number)
└── Checksum

Page Data (~8KB - 24 bytes)
├── Data records or index entries
└── Free space
```

## 🔄 Database Operations

### Creating Multiple Databases

```bash
# Create different databases for different projects
cargo run users.db
cargo run products.db
cargo run orders.db
```

### Database Backup (Manual)

```bash
# Simple file copy
cp my_database.db my_database_backup.db

# With WAL file (if enabled)
cp my_database.db my_database_backup.db
cp my_database.wal my_database_backup.wal
```

### Database Migration

```bash
# Move database to different location
mv my_database.db /new/location/my_database.db

# Update path in application
cargo run /new/location/my_database.db
```

## ⚠️ Important Notes

### File Permissions
- Database files need read/write permissions
- WAL files should be on the same filesystem as the main DB file

### File Size Management
- Database files grow as you add data
- Consider disk space requirements
- Use VACUUM operations when implemented

### Crash Recovery
- If WAL is enabled, database can recover from crashes
- Ensure proper shutdown before moving files
- Keep regular backups

## 🔍 Troubleshooting

### "No such file or directory" Error
```bash
# This error now only occurs if:
# 1. Directory doesn't exist
# 2. No write permissions in directory

# Solution: Create directory and check permissions
mkdir -p /path/to/databases
chmod 755 /path/to/databases
cargo run /path/to/databases/my.db
```

### File Locked Error
```bash
# Another process is using the database
# Solution: Close other instances or wait for completion
pkill rustgresql
cargo run my.db
```

### Corrupted Database
```bash
# Database file is corrupted (rare)
# Solution: Start with fresh database
mv my_database.db my_database.db.corrupted
cargo run my_database.db  # Creates new file
```

## 🎉 Success!

You've successfully created a RustgreSQL database file! The database is now ready for:

- ✅ Data storage and retrieval (when SQL execution is implemented)
- ✅ Transaction management
- ✅ ACID compliance
- ✅ Crash recovery
- ✅ Multiple concurrent access

**Next Steps:**
1. Use the interactive REPL to explore commands
2. Check out the documentation for API usage
3. Wait for SQL execution features in future releases

---

**Example Usage Session:**
```bash
$ cargo run my_app.db
RustgreSQL v0.1.0
Type 'help' for commands or 'exit' to quit.
rustgresql> help
Available commands:
  help     - Show this help message
  status   - Show database status
  exit     - Exit the program
rustgresql> status
Database is running
Data file: my_app.db
rustgresql> exit
Goodbye!

$ ls -la my_app.db
-rw-r--r-- 1 user user 8192 Nov 14 11:20 my_app.db
```

Your database file is ready! 🚀