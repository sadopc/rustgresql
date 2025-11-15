# RustgreSQL Developer Guide

## Codebase Architecture

This guide explains the internal architecture and implementation details of RustgreSQL for developers who want to understand, contribute to, or extend the database system.

## Project Structure

```
src/
├── lib.rs              # Main library entry point and public API
├── main.rs             # CLI application entry point
├── error.rs            # Error types and handling
├── storage/            # Storage engine implementation
│   ├── mod.rs
│   ├── buffer.rs       # Buffer pool manager
│   ├── file_manager.rs # File I/O operations
│   ├── btree.rs        # B-Tree implementation
│   └── page.rs         # Page structure and management
├── transaction/        # Transaction management
│   ├── mod.rs
│   ├── manager.rs      # Transaction coordinator
│   ├── wal.rs          # Write-ahead logging
│   ├── mvcc.rs         # Multi-version concurrency control
│   └── lock.rs         # Lock manager
├── catalog/            # System catalog and metadata
│   ├── mod.rs
│   ├── table.rs        # Table management
│   ├── index.rs        # Index management
│   └── schema.rs       # Schema management
├── sql/                # SQL processing
│   ├── mod.rs
│   ├── parser.rs       # SQL parsing
│   └── planner.rs      # Query planning
├── executor/           # Query execution
│   ├── mod.rs
│   ├── operators.rs    # Execution operators
│   └── planner.rs      # Execution planning
└── types/              # Type system
    ├── mod.rs
    ├── data_type.rs    # Data type definitions
    ├── value.rs        # Value representation
    └── convert.rs      # Type conversion
```

## Core Components Deep Dive

### 1. Database Core (`lib.rs`)

The `Database` struct is the main entry point:

```rust
pub struct Database {
    pub config: Config,
    buffer_manager: Arc<storage::BufferPoolManager>,
    catalog_manager: Arc<catalog::CatalogManager>,
}
```

**Key Responsibilities:**
- Initialize storage and transaction systems
- Provide public API for database operations
- Manage configuration and lifecycle

### 2. Storage Engine (`storage/`)

#### Buffer Pool Manager (`buffer.rs`)

```rust
pub struct BufferPoolManager {
    buffer_pool: BufferPool,
    file_manager: Arc<Mutex<dyn FileManager + Send>>,
}
```

**Purpose**: Caches database pages in memory to reduce disk I/O

**Key Methods**:
- `fetch_page(page_id)` - Load page into buffer pool
- `flush_page(page_id)` - Write dirty page to disk
- `evict_page()` - Remove page from buffer pool (LRU)

#### B-Tree Implementation (`btree.rs`)

```rust
pub struct BTree {
    root_page_id: PageId,
    buffer_manager: Arc<BufferPoolManager>,
}
```

**Purpose**: Provides ordered data storage and efficient range queries

**Structure**:
- Internal nodes contain keys and child page pointers
- Leaf nodes contain actual key-value pairs
- Supports insert, search, delete operations

#### File Manager (`file_manager.rs`)

```rust
pub trait FileManager: Send + Sync {
    fn read_page(&self, page_id: PageId) -> Result<Vec<u8>>;
    fn write_page(&self, page_id: PageId, data: &[u8]) -> Result<()>;
    fn allocate_page(&self) -> Result<PageId>;
}
```

**Purpose**: Abstracts disk I/O operations

**Implementation**: `DefaultFileManager` handles actual file operations

### 3. Transaction System (`transaction/`)

#### Transaction Manager (`manager.rs`)

```rust
pub struct TransactionManager {
    next_transaction_id: Arc<Mutex<TransactionId>>,
    active_transactions: Arc<Mutex<HashMap<TransactionId, Transaction>>>,
    wal: Option<Arc<Mutex<WALManager>>>,
    mvcc: Arc<Mutex<MVCCManager>>,
}
```

**ACID Properties Implementation**:
- **Atomicity**: Through two-phase commit
- **Consistency**: Through validation in transaction manager
- **Isolation**: Through MVCC and locking
- **Durability**: Through WAL (Write-Ahead Logging)

#### Transaction Structure

```rust
pub struct Transaction {
    pub id: TransactionId,
    pub state: TransactionState,
    pub snapshot: Option<Snapshot>,
    pub isolation_level: IsolationLevel,
    pub start_ts: u64,
    pub last_lsn: Option<LSN>,
    pub modified_pages: HashMap<PageId, Vec<u8>>,
}
```

#### Write-Ahead Logging (`wal.rs`)

```rust
pub struct WALRecord {
    pub record_type: WALRecordType,
    pub transaction_id: TransactionId,
    pub lsn: LSN,
    pub page_id: Option<PageId>,
    pub data: Vec<u8>,
}
```

**Purpose**: Ensures durability by logging all changes before applying them

**Recovery Process**:
1. Scan WAL file after crash
2. Reapply committed transactions
3. Rollback uncommitted transactions

#### MVCC (`mvcc.rs`)

```rust
pub struct VersionChain {
    pub record_id: RecordId,
    pub versions: Vec<RecordVersion>,
}

pub struct RecordVersion {
    pub transaction_id: TransactionId,
    pub timestamp: u64,
    pub data: Vec<u8>,
    pub deleted: bool,
}
```

**Purpose**: Allows concurrent reads without blocking writes

**How it Works**:
1. Each modification creates a new version
2. Readers see consistent snapshot from their start time
3. Old versions cleaned up during vacuum

### 4. Catalog System (`catalog/`)

#### Catalog Manager (`mod.rs`)

```rust
pub struct CatalogManager {
    tables: Arc<Mutex<HashMap<String, TableDef>>>,
    schemas: Arc<Mutex<HashMap<String, SchemaDef>>>,
    next_table_id: Arc<Mutex<u64>>,
}
```

**System Tables**:
- `pg_tables` - Table definitions
- `pg_columns` - Column definitions
- `pg_indexes` - Index definitions

#### Table Definition

```rust
pub struct TableDef {
    pub table_id: u64,
    pub name: String,
    pub schema_id: u64,
    pub columns: Vec<ColumnDef>,
    pub indexes: Vec<IndexDef>,
}
```

### 5. Type System (`types/`)

#### Data Types (`data_type.rs`)

```rust
pub enum DataTypeKind {
    SmallInt, Integer, BigInt,
    Real, DoublePrecision,
    Numeric(usize, usize),
    Char(usize), Varchar(usize), Text,
    Boolean,
    Date, Time, Timestamp,
    Array(Box<DataTypeKind>),
    // ... more types
}
```

#### Value Representation (`value.rs`)

```rust
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Boolean(bool),
    Date(NaiveDate),
    Array(Vec<Value>),
    // ... more types
}
```

### 6. SQL Processing (`sql/`)

#### Parser (`parser.rs`)

```rust
pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
}
```

**Supported Statements**:
- `SELECT` - Basic parsing (execution pending)
- `INSERT` - Basic parsing (execution pending)
- `CREATE TABLE` - Basic parsing (execution pending)

**Parsing Process**:
1. Lexical analysis (tokens)
2. Syntax analysis (AST)
3. Semantic analysis (validation)

### 7. Query Execution (`executor/`)

#### Execution Plan (`planner.rs`)

```rust
pub enum PlanNode {
    Scan { table_name: String, filter: Option<Expression> },
    Project { columns: Vec<(String, Expression) },
    Filter { condition: Expression },
    Aggregate { group_by: Vec<String>, aggregates: Vec<AggregateFunction> },
}
```

**Execution Pipeline**:
1. Parse SQL → AST
2. Plan → Physical execution plan
3. Execute → Results

## DDL Implementation Architecture

RustgreSQL provides comprehensive Data Definition Language (DDL) support for creating, modifying, and dropping database objects. The DDL implementation is fully integrated with the parser, execution engine, catalog system, storage layer, transaction system, and WAL logging.

### DDL Parsing Pipeline

The DDL parsing process extends the core SQL parser with keyword and statement recognition:

#### 1. Lexer Enhancements

The lexer recognizes DDL-specific keywords:

```rust
// Keywords for DDL statements
DROP, ALTER, CREATE, REFERENCES, CHECK, IF, EXISTS,
CONSTRAINT, FOREIGN, ADD, COLUMN, RENAME, TO, UNIQUE
```

#### 2. AST Structures for DDL

The Abstract Syntax Tree includes comprehensive DDL node types:

```rust
pub enum Statement {
    CreateTable(CreateTableStatement),
    DropTable(DropTableStatement),
    DropIndex(DropIndexStatement),
    AlterTable(AlterTableStatement),
    CreateIndex(CreateIndexStatement),
    // ... other statements
}

pub struct CreateTableStatement {
    pub table_name: String,
    pub if_not_exists: bool,
    pub columns: Vec<ColumnDef>,
    pub table_constraints: Vec<TableConstraint>,
}

pub struct DropTableStatement {
    pub table_name: String,
    pub if_exists: bool,
}

pub struct AlterTableStatement {
    pub table_name: String,
    pub operations: Vec<AlterOperation>,
}

pub enum AlterOperation {
    AddColumn { column_def: ColumnDef },
    DropColumn { column_name: String },
    AddConstraint { constraint: TableConstraint },
    DropConstraint { constraint_name: String },
    RenameColumn { old_name: String, new_name: String },
    RenameTable { new_name: String },
}
```

#### 3. Constraint Representation

Constraints are represented at both column and table levels:

```rust
pub enum ColumnConstraint {
    NotNull,
    Null,
    Default(String),
    PrimaryKey,
    Unique,
    Check(Expression),
    References { table: String, column: Option<String> },
}

pub enum TableConstraint {
    PrimaryKey { columns: Vec<String>, name: Option<String> },
    ForeignKey { columns: Vec<String>, ref_table: String,
                 ref_columns: Vec<String>, name: Option<String> },
    Unique { columns: Vec<String>, name: Option<String> },
    Check { condition: Expression, name: Option<String> },
}
```

### DDL Execution Engine

The execution engine handles all DDL statement execution with proper error handling and transaction support.

#### 1. CREATE TABLE Execution

```
parse_create_table()
    ↓
validate_columns()
    ↓
validate_constraints()
    ↓
create_table_in_catalog()
    ↓
create_physical_storage()
    ↓
create_indexes()
    ↓
log_to_wal()
    ↓
Transaction Commit/Rollback
```

**Key features**:
- `IF NOT EXISTS` clause prevents errors if table exists
- Automatic index creation for PRIMARY KEY and UNIQUE constraints
- Column constraint validation during table creation
- Table constraint validation for foreign keys and composite keys

#### 2. DROP TABLE Execution

```
parse_drop_table()
    ↓
check_if_table_exists()
    ↓
validate_no_dependencies()
    ↓
remove_from_catalog()
    ↓
delete_physical_storage()
    ↓
drop_indexes()
    ↓
log_to_wal()
    ↓
Transaction Commit/Rollback
```

**Dependency checking**:
- Prevents dropping tables referenced by FOREIGN KEY constraints
- Handles system-generated indexes
- Validates no active transactions hold locks

#### 3. ALTER TABLE Execution

```
parse_alter_table()
    ↓
validate_table_exists()
    ↓
for each operation:
    ├─ execute_alter_operation()
    ├─ validate_operation()
    ├─ apply_catalog_changes()
    ├─ update_physical_storage()
    └─ handle_dependent_objects()
    ↓
log_all_changes_to_wal()
    ↓
Transaction Commit/Rollback
```

**Supported operations**:
- `ADD COLUMN`: Add new columns with optional defaults
- `DROP COLUMN`: Remove columns with dependency checking
- `ADD CONSTRAINT`: Add named constraints to existing tables
- `DROP CONSTRAINT`: Remove constraints with validation
- `RENAME COLUMN`: Rename column (updates metadata and defaults)
- `RENAME TABLE`: Rename table (updates catalog)

### Catalog Integration

The catalog system stores DDL metadata:

```rust
pub struct TableDef {
    pub table_id: u64,
    pub name: String,
    pub schema_id: u64,
    pub columns: Vec<ColumnDef>,
    pub constraints: Vec<ConstraintDef>,
    pub indexes: Vec<IndexDef>,
}

pub struct ConstraintDef {
    pub constraint_id: u64,
    pub name: Option<String>,
    pub constraint_type: ConstraintType,
    pub table_id: u64,
    pub columns: Vec<String>,
    pub referenced_table: Option<String>,
    pub referenced_columns: Option<Vec<String>>,
}
```

**Catalog operations**:
- `add_table()`: Register new table definition
- `drop_table()`: Remove table metadata
- `add_column()`: Add column to table definition
- `drop_column()`: Remove column from definition
- `add_constraint()`: Register constraint
- `drop_constraint()`: Remove constraint registration

### Storage Layer Integration

The storage engine provides physical DDL support:

```rust
// Storage operations for DDL
pub trait StorageEngine {
    fn create_table(&self, table_def: &TableDef) -> Result<()>;
    fn drop_table(&self, table_id: u64) -> Result<()>;
    fn add_column(&self, table_id: u64, column: &ColumnDef) -> Result<()>;
    fn drop_column(&self, table_id: u64, column_name: &str) -> Result<()>;
    fn create_index(&self, index_def: &IndexDef) -> Result<()>;
    fn drop_index(&self, index_id: u64) -> Result<()>;
}
```

**Schema evolution support**:
- Dynamic column addition for existing data
- Safe column removal with validation
- Index creation and deletion
- Constraint-backed storage optimization

### WAL Integration for DDL

Write-Ahead Logging ensures durability of DDL operations:

```rust
pub enum DDLLogEntry {
    CreateTable { table_id: u64, table_def: TableDef },
    DropTable { table_id: u64 },
    AddColumn { table_id: u64, column_def: ColumnDef },
    DropColumn { table_id: u64, column_name: String },
    AddConstraint { constraint_def: ConstraintDef },
    DropConstraint { constraint_id: u64 },
}
```

**DDL Recovery**:
1. On startup, scan WAL for committed DDL operations
2. Reapply schema changes to memory structures
3. Verify physical storage matches logical schema
4. Rollback incomplete DDL transactions

### MVCC Integration for Schema Changes

DDL operations are integrated with MVCC:

```
DDL Operation Start
    ↓
Create new schema snapshot
    ↓
Apply changes to snapshot
    ↓
All subsequent transactions see new schema
    ↓
Old schema versions cleaned up during vacuum
```

**Schema visibility rules**:
- DDL operations are transactional
- Readers see consistent schema within snapshot
- Concurrent DDL operations are serialized with locking
- Schema changes have proper isolation

### Error Handling for DDL

Comprehensive error handling with PostgreSQL-compatible codes:

```rust
pub enum DdlError {
    // Table errors (4000-4099)
    TableNotFound(String),
    TableAlreadyExists(String),
    TableInUse(String),
    TableDependencyExists(String),

    // Column errors (4100-4199)
    ColumnNotFound(String),
    ColumnAlreadyExists(String),
    ColumnInUse(String),
    InvalidColumnDefinition(String),

    // Constraint errors (4200-4299)
    ConstraintNotFound(String),
    ConstraintAlreadyExists(String),
    ConstraintViolation(String),
    InvalidConstraintDefinition(String),

    // ... more error types
}
```

**Error recovery**:
- Transaction rollback for failed operations
- Detailed error messages with context
- Recovery suggestions in error objects
- SQLSTATE codes for client compatibility

### DDL Execution Flow

Complete flow for a DDL operation:

```
1. PARSE PHASE
   - Lexical analysis (tokens)
   - Syntax analysis (AST)
   - Semantic validation

2. VALIDATION PHASE
   - Check table existence
   - Validate column definitions
   - Check constraint semantics
   - Verify no dependencies (for DROP)

3. EXECUTION PHASE
   - Begin transaction
   - Update catalog
   - Update physical storage
   - Create indexes if needed
   - Write to WAL

4. COMMIT PHASE
   - Mark transaction as committed
   - Make changes visible
   - Update schema snapshots

5. ERROR HANDLING
   - Rollback if any phase fails
   - Release locks
   - Restore previous state
```

## Constraint Validation Architecture

The constraint validation system ensures data integrity throughout the database lifecycle.

### Constraint Validation Framework

Constraints are validated at multiple points in the data lifecycle:

#### 1. Parse-Time Validation

```
SQL Statement
    ↓
Lexer (syntax check)
    ↓
Parser (grammar check)
    ↓
Result: Syntax errors detected early
```

#### 2. Create-Time Validation

```
CREATE TABLE statement
    ↓
Validate column definitions
    ├─ Check data types
    ├─ Validate constraint syntax
    └─ Check constraint compatibility
    ↓
Validate table constraints
    ├─ Check referenced tables exist
    ├─ Detect circular references
    └─ Validate constraint semantics
    ↓
Create table with validated constraints
```

#### 3. Runtime Validation

```
INSERT/UPDATE statement
    ↓
For each constraint:
    ├─ PRIMARY KEY: Uniqueness check
    ├─ FOREIGN KEY: Referential integrity
    ├─ UNIQUE: Uniqueness (NULL allowed)
    ├─ CHECK: Expression evaluation
    └─ NOT NULL: Value presence check
    ↓
All constraints satisfied → Operation succeeds
Any constraint violated → Operation fails with error
```

### Constraint Types and Validation

#### PRIMARY KEY Validation

```rust
fn validate_primary_key(columns: &[String], values: &[Value]) -> bool {
    // Check: No NULL values allowed
    if values.iter().any(|v| matches!(v, Value::Null)) {
        return false;
    }

    // Check: Value must be unique in table
    // Uses existing PRIMARY KEY index for efficiency
    index.contains(values)
}
```

**Optimization**: Uses B-Tree index on PRIMARY KEY for O(log n) lookup

#### FOREIGN KEY Validation

```rust
fn validate_foreign_key(
    table: &str,
    column: &str,
    ref_table: &str,
    ref_column: &str,
    value: &Value
) -> bool {
    if matches!(value, Value::Null) {
        return true;  // NULLs are allowed in FK columns
    }

    // Check: Value must exist in referenced table
    ref_table_index.contains((ref_column, value))
}
```

**Features**:
- NULL values allowed (standard SQL)
- CASCADE delete supported
- RESTRICT enforcement (default)
- Circular reference detection

#### UNIQUE Validation

```rust
fn validate_unique(column: &str, value: &Value) -> bool {
    if matches!(value, Value::Null) {
        return true;  // Multiple NULLs allowed
    }

    // Check: Value must not exist in column
    unique_index.contains(value)
}
```

**Three-valued logic**:
- NULL is not equal to NULL (standard SQL)
- Multiple NULLs are allowed
- Uses UNIQUE index for efficiency

#### CHECK Validation

```rust
fn validate_check(condition: &Expression, row: &Row) -> bool {
    // Evaluate expression against row values
    // Result: true (passes), false (fails), or NULL (unknown)

    match evaluate_expression(condition, row) {
        Value::Boolean(true) => true,   // Constraint satisfied
        Value::Boolean(false) => false, // Constraint violated
        Value::Null => true,            // Unknown (NULL) treated as satisfied
        _ => false,                     // Type error
    }
}
```

**Expression support**:
- Comparison operators: `=`, `!=`, `<`, `>`, `<=`, `>=`
- Logical operators: `AND`, `OR`, `NOT`
- Arithmetic operators: `+`, `-`, `*`, `/`
- String functions: `LIKE`, `LENGTH`, etc.

### Index-Backed Constraint Enforcement

Constraints are backed by indexes for efficiency:

```
PRIMARY KEY → B-Tree index (unique, non-null)
UNIQUE      → B-Tree index (unique, null-allowed)
FOREIGN KEY → B-Tree index on referenced column
Regular Col → No automatic index
```

**Benefits**:
- Fast constraint validation (O(log n) instead of O(n))
- Automatic index creation with constraint
- Shared index usage for efficiency

### Constraint Mapping: AST to Catalog

Constraints defined in SQL (AST) are mapped to catalog representations:

```rust
// AST constraint (parsed from SQL)
ColumnConstraint::Check(expression)

// Mapped to catalog
ConstraintDef {
    constraint_type: ConstraintType::Check,
    expression: Some(expression),
    ...
}

// Validation: expression is stored and evaluated at runtime
```

**Conversion process**:
1. Parse constraint from SQL
2. Validate constraint syntax
3. Map to catalog representation
4. Store in metadata system
5. Create index if needed
6. Register in constraint validator

### Constraint Dependency Graph

The system tracks constraint dependencies for proper ordering:

```
Foreign Key A → Table B
Foreign Key B → Table C
→ Dependency path: A → B → C

Benefits:
- Circular dependency detection
- Optimal drop order
- Safe cascade deletion
```

### Error Handling for Constraint Violations

When constraints are violated:

```
Constraint Violation Detected
    ↓
Identify constraint
    ↓
Generate error message with:
    ├─ Constraint name
    ├─ Violated column(s)
    ├─ Reason (duplicate/missing ref/failed check)
    └─ Suggested fix
    ↓
Return error to application
    ↓
Transaction rolls back
```

**Example error response**:
```json
{
    "error_code": 4202,
    "sqlstate": "23000",
    "message": "Constraint violation: fk_orders_customers",
    "context": {
        "constraint": "fk_orders_customers",
        "table": "orders",
        "column": "customer_id",
        "value": "999",
        "reason": "No matching customer with id 999"
    }
}
```

### Performance Considerations

#### Constraint Validation Performance

| Constraint Type | Validation Time | Optimization |
|-----------------|-----------------|--------------|
| PRIMARY KEY | O(log n) | Clustered index |
| UNIQUE | O(log n) | B-Tree index |
| FOREIGN KEY | O(log n) | Indexed reference |
| CHECK | O(1) - O(n) | Expression complexity |
| NOT NULL | O(1) | Direct value check |

#### Large Table Implications

- Add PRIMARY KEY/UNIQUE: Full table scan for uniqueness
- Add FOREIGN KEY: Reference table scan for validity
- Add CHECK: Full table scan for condition evaluation
- Operations should occur during low-traffic periods

#### Optimization Strategies

1. **Pre-aggregate checks**: Validate before bulk operations
2. **Batch validation**: Check multiple rows efficiently
3. **Defer constraints**: In transactions, validate at commit
4. **Partial validation**: For large tables, spot-check samples
5. **Parallel validation**: Use multiple cores for checks

## Key Algorithms and Data Structures

### B-Tree Implementation

**Node Structure**:
```
Internal Node: [keys][child_pointers]
Leaf Node:     [keys][values]
```

**Operations**:
- **Search**: Navigate down tree using key comparisons
- **Insert**: Find leaf, insert, split if full
- **Delete**: Find key, remove, merge if underfull

### MVCC Visibility Rules

```rust
fn is_version_visible(version: &RecordVersion, snapshot: &Snapshot) -> bool {
    version.timestamp <= snapshot.timestamp &&
    !snapshot.active_transactions.contains(&version.transaction_id)
}
```

### LRU Eviction

```rust
fn evict_lru(&mut self) {
    // Find least recently used page
    // Write to disk if dirty
    // Remove from buffer pool
}
```

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_btree_insert() {
        let btree = BTree::new(/* ... */);
        btree.insert(key, value);
        assert_eq!(btree.search(key), Some(value));
    }
}
```

### Integration Tests

```rust
#[test]
fn test_transaction_isolation() {
    let db = Database::new(config);
    let tx1 = db.begin_transaction();
    let tx2 = db.begin_transaction();
    // Test isolation behavior
}
```

### Property-Based Tests

```rust
#[quickcheck]
fn prop_btree_consistency(operations: Vec<Operation>) -> bool {
    // Verify B-Tree invariants after random operations
}
```

## Performance Considerations

### Buffer Pool Tuning

- **Size**: Larger pools reduce disk I/O but use more memory
- **Eviction Policy**: LRU is simple, may need Clock algorithm
- **Prefetching**: Could improve sequential scan performance

### B-Tree Optimization

- **Node Size**: Balance between memory usage and I/O efficiency
- **Bulk Loading**: Specialized algorithms for initial data load
- **Compression**: Could reduce storage requirements

### Transaction Optimization

- **Lock Granularity**: Row-level vs page-level vs table-level
- **Deadlock Detection**: Timeout-based vs cycle detection
- **Checkpoint Frequency**: Balance between recovery time and runtime overhead

## Future Development Areas

### 1. SQL Execution Engine
- Implement full SELECT execution
- Add INSERT/UPDATE/DELETE support
- Query optimization (cost-based)

### 2. Advanced Indexing
- Hash indexes for equality queries
- GiST indexes for spatial data
- Partial indexes

### 3. Network Protocol
- PostgreSQL wire protocol
- Connection pooling
- Authentication

### 4. Storage Optimization
- Columnar storage for analytics
- Compression algorithms
- Partitioning strategies

## Contributing Guidelines

### Code Style

1. Use `cargo fmt` for consistent formatting
2. Run `cargo clippy` to catch common issues
3. Follow Rust naming conventions
4. Add comprehensive documentation

### Adding New Features

1. **Design**: Create detailed design document
2. **Implementation**: Write code with tests
3. **Documentation**: Update relevant documentation
4. **Review**: Submit pull request for review

### Testing Requirements

- Unit tests for all new functions
- Integration tests for major features
- Property-based tests for algorithms
- Performance benchmarks for critical paths

## Debugging Tools

### Logging

```rust
use log::{debug, info, warn, error};

debug!("Detailed debugging information");
info!("General information");
warn!("Warning conditions");
error!("Error conditions");
```

Enable with:
```bash
RUST_LOG=debug cargo run
```

### Profiling

```bash
# CPU profiling
cargo build --release
perf record --call-graph=dwarf ./target/release/rustgresql
perf report

# Memory profiling
valgrind --tool=massif ./target/release/rustgresql
```

---

This guide provides a comprehensive overview of the RustgreSQL codebase. For specific implementation details, refer to the individual module documentation and source code comments.