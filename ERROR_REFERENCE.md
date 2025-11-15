# DDL Error Reference

This document provides comprehensive documentation of all error codes for DDL (Data Definition Language) operations in RustgreSQL. All errors use PostgreSQL-compatible SQLSTATE codes for compatibility with database clients and tools.

## Error Code Categories

RustgreSQL organizes DDL errors into the following categories:

| Range | Category | SQLSTATE Prefix |
|-------|----------|-----------------|
| 4000-4099 | Table Errors | 42P.. |
| 4100-4199 | Column Errors | 42P.. |
| 4200-4299 | Constraint Errors | 23... |
| 4300-4399 | Index Errors | 42P.. |
| 4400-4499 | Schema Errors | 3F... |
| 4500-4599 | Dependency Errors | 2BP.. |
| 4600-4699 | Transaction Errors | 42... |

## Table Errors (4000-4099)

Errors related to table operations (CREATE TABLE, DROP TABLE, ALTER TABLE).

### Error 4000: TableNotFound
**SQLSTATE**: 42P01
**Message**: "Table '{table_name}' not found"
**When it occurs**: Attempting to reference a table that doesn't exist
**Common causes**:
- Typo in table name
- Table was dropped
- Table in wrong schema

**Example**:
```sql
-- This will fail if users table doesn't exist
DROP TABLE users;
-- Error: Table 'users' not found
```

**Recovery**:
- Use `CREATE TABLE IF NOT EXISTS` to avoid errors
- Check table name spelling
- Use `IF EXISTS` clause: `DROP TABLE IF EXISTS users`

---

### Error 4001: TableAlreadyExists
**SQLSTATE**: 42P06
**Message**: "Table '{table_name}' already exists"
**When it occurs**: Attempting to create a table that already exists
**Common causes**:
- Table was already created
- Duplicate CREATE TABLE statement

**Example**:
```sql
CREATE TABLE users (id INTEGER PRIMARY KEY);
CREATE TABLE users (id INTEGER PRIMARY KEY);
-- Error: Table 'users' already exists
```

**Recovery**:
- Use `CREATE TABLE IF NOT EXISTS` to skip if table exists
- Check if table creation is idempotent
```sql
CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY);
```

---

### Error 4002: TableInUse
**SQLSTATE**: 40P00
**Message**: "Table '{table_name}' is in use"
**When it occurs**: Attempting to drop or modify a table while it's being accessed by another transaction
**Common causes**:
- Another connection is reading from the table
- Active transaction has locks on the table
- Concurrent DDL operations

**Example**:
```sql
-- Session 1: Reads from users table
BEGIN TRANSACTION;
SELECT * FROM users WHERE id = 1;

-- Session 2: Tries to drop the table
DROP TABLE users;
-- Error: Table 'users' is in use
```

**Recovery**:
- Wait for other transactions to complete
- Use application connection pooling to minimize conflicts
- Set statement timeout: `SET statement_timeout = 30000`
- Consider timing of DDL operations during low-traffic periods

---

### Error 4003: TableDependencyExists
**SQLSTATE**: 2BP04
**Message**: "Cannot drop table '{table_name}' because other objects depend on it"
**When it occurs**: Attempting to drop a table that is referenced by constraints, views, or indexes
**Common causes**:
- Foreign key references from other tables
- Indexes on the table
- Views referencing the table

**Example**:
```sql
-- Create customers and orders tables with FK
CREATE TABLE customers (customer_id INTEGER PRIMARY KEY);
CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER REFERENCES customers(customer_id)
);

-- Try to drop customers table
DROP TABLE customers;
-- Error: Cannot drop table 'customers' because other objects depend on it
```

**Recovery**:
- Drop dependent objects first:
```sql
DROP TABLE orders;  -- Drop referencing table first
DROP TABLE customers;  -- Then drop referenced table
```
- Or use CASCADE (if supported):
```sql
-- Note: Check if CASCADE is supported in your version
```

---

### Error 4004: ColumnNotFound
**SQLSTATE**: 42703
**Message**: "Column '{column_name}' not found in table '{table_name}'"
**When it occurs**: Referencing a column that doesn't exist in the specified table
**Common causes**:
- Column name typo
- Column was dropped
- Wrong table name

**Example**:
```sql
ALTER TABLE users DROP COLUMN email_address;
-- Error: Column 'email_address' not found in table 'users'
```

**Recovery**:
- Verify column name: `SELECT * FROM pg_columns WHERE table_name = 'users'`
- Check if column was already dropped
- Use exact column name spelling

---

### Error 4005: ColumnAlreadyExists
**SQLSTATE**: 42701
**Message**: "Column '{column_name}' already exists in table '{table_name}'"
**When it occurs**: Attempting to add a column with the same name as an existing column
**Common causes**:
- Column was already added
- Duplicate migration execution

**Example**:
```sql
ALTER TABLE users ADD COLUMN email VARCHAR(255);
ALTER TABLE users ADD COLUMN email VARCHAR(255);
-- Error: Column 'email' already exists in table 'users'
```

**Recovery**:
- Check if column already exists before adding
- Use idempotent migrations
- Verify migration has been executed

---

### Error 4006: ColumnInUse
**SQLSTATE**: 2BP05
**Message**: "Cannot drop column '{column_name}' because it is in use"
**When it occurs**: Attempting to drop a column that is used in constraints, indexes, or other objects
**Common causes**:
- Column is part of PRIMARY KEY
- Column is referenced by FOREIGN KEY
- Column is indexed
- Column used in CHECK constraint

**Example**:
```sql
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    email VARCHAR(255) UNIQUE
);

-- Try to drop user_id (part of PRIMARY KEY)
ALTER TABLE users DROP COLUMN user_id;
-- Error: Cannot drop column 'user_id' because it is in use

-- Try to drop email (has UNIQUE index)
ALTER TABLE users DROP COLUMN email;
-- Error: Cannot drop column 'email' because it is in use
```

**Recovery**:
- Drop constraints/indexes first, then drop column
```sql
ALTER TABLE users DROP CONSTRAINT uk_email;  -- Drop unique constraint
ALTER TABLE users DROP COLUMN email;  -- Now column can be dropped
```

---

### Error 4007: InvalidColumnDefinition
**SQLSTATE**: 42601
**Message**: "Invalid column definition: {reason}"
**When it occurs**: Column definition has invalid syntax or invalid constraint combination
**Common causes**:
- Invalid data type
- Conflicting constraints
- Syntax error in constraint definition

**Example**:
```sql
-- Invalid: PRIMARY KEY and UNIQUE together (redundant)
CREATE TABLE users (
    id INTEGER PRIMARY KEY UNIQUE
);  -- Warning, though not necessarily an error

-- Invalid: NULL and NOT NULL together
CREATE TABLE users (
    id INTEGER NOT NULL NULL
);
-- Error: Invalid column definition
```

**Recovery**:
- Review column definition syntax
- Check data type spelling
- Remove conflicting constraints

## Constraint Errors (4200-4299)

Errors related to constraint definition and validation.

### Error 4200: ConstraintNotFound
**SQLSTATE**: 42704
**Message**: "Constraint '{constraint_name}' not found"
**When it occurs**: Attempting to drop a constraint that doesn't exist
**Common causes**:
- Constraint name typo
- Constraint was already dropped
- Wrong constraint name format

**Example**:
```sql
ALTER TABLE users DROP CONSTRAINT uk_email;
-- Error if constraint doesn't exist with that exact name
```

**Recovery**:
- Use `IF EXISTS` clause:
```sql
ALTER TABLE users DROP CONSTRAINT IF EXISTS uk_email;
```
- List existing constraints to find correct name

---

### Error 4201: ConstraintAlreadyExists
**SQLSTATE**: 42710
**Message**: "Constraint '{constraint_name}' already exists"
**When it occurs**: Attempting to create a constraint with the same name
**Common causes**:
- Constraint was already added
- Duplicate migration

**Example**:
```sql
ALTER TABLE users ADD CONSTRAINT uk_email UNIQUE (email);
ALTER TABLE users ADD CONSTRAINT uk_email UNIQUE (email);
-- Error: Constraint 'uk_email' already exists
```

**Recovery**:
- Drop existing constraint first
- Use different constraint name
- Check migration scripts for duplicates

---

### Error 4202: ConstraintViolation
**SQLSTATE**: 23000
**Message**: "Constraint violation: {constraint_name} - {reason}"
**When it occurs**: Data violates constraint during INSERT/UPDATE
**Common causes**:
- PRIMARY KEY violation (duplicate value)
- FOREIGN KEY violation (referenced value doesn't exist)
- UNIQUE violation (duplicate non-null value)
- CHECK violation (value doesn't satisfy condition)

**Example**:
```sql
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    email VARCHAR(255) UNIQUE
);

INSERT INTO users VALUES (1, 'john@example.com');
INSERT INTO users VALUES (1, 'jane@example.com');
-- Error: ConstraintViolation - PRIMARY KEY: Duplicate key value
```

**Recovery**:
- Verify data meets constraint requirements
- Check for duplicate values
- Ensure referenced values exist
- Validate against CHECK conditions

---

### Error 4203: InvalidConstraintDefinition
**SQLSTATE**: 42601
**Message**: "Invalid constraint definition: {reason}"
**When it occurs**: Constraint syntax is invalid or logically impossible
**Common causes**:
- Invalid CHECK expression
- Circular FOREIGN KEY references
- Invalid column list for constraint
- Missing referenced table/columns

**Example**:
```sql
-- Invalid: CHECK with invalid condition
ALTER TABLE orders ADD CONSTRAINT ck_dates CHECK (order_date >);
-- Error: Invalid constraint definition

-- Invalid: FOREIGN KEY to non-existent table
ALTER TABLE orders ADD CONSTRAINT fk_customer
    FOREIGN KEY (customer_id) REFERENCES non_existent_table(id);
-- Error: Invalid constraint definition
```

**Recovery**:
- Fix constraint syntax
- Ensure referenced tables and columns exist
- Validate CHECK expressions
- Check circular dependencies

## Index Errors (4300-4399)

Errors related to index operations.

### Error 4300: IndexNotFound
**SQLSTATE**: 42704
**Message**: "Index '{index_name}' not found"
**When it occurs**: Attempting to drop an index that doesn't exist
**Common causes**:
- Index name typo
- Index was already dropped
- Wrong index name

**Example**:
```sql
DROP INDEX idx_users_email;
-- Error if index doesn't exist with that exact name
```

**Recovery**:
- Use `IF EXISTS` clause:
```sql
DROP INDEX IF EXISTS idx_users_email;
```

---

### Error 4301: IndexAlreadyExists
**SQLSTATE**: 42710
**Message**: "Index '{index_name}' already exists"
**When it occurs**: Attempting to create an index that already exists
**Common causes**:
- Index was already created
- Automatic index creation (PRIMARY KEY, UNIQUE)

**Example**:
```sql
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_email ON users(email);
-- Error: Index 'idx_users_email' already exists
```

**Recovery**:
- Use `IF NOT EXISTS` clause:
```sql
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
```
- Drop existing index first

---

### Error 4302: IndexInUse
**SQLSTATE**: 40P00
**Message**: "Index '{index_name}' is in use"
**When it occurs**: Attempting to drop an index while it's being used
**Common causes**:
- Index is backing a constraint (PRIMARY KEY, UNIQUE)
- Active query is using the index
- System-generated constraint index

**Example**:
```sql
CREATE TABLE users (user_id INTEGER PRIMARY KEY);
DROP INDEX idx_users_pk;  -- System-generated index
-- Error: Cannot drop system-generated index
```

**Recovery**:
- Drop the constraint instead of the index
- Wait for active queries to complete

## Schema Errors (4400-4499)

Errors related to schema management.

### Error 4400: SchemaNotFound
**SQLSTATE**: 3F000
**Message**: "Schema '{schema_name}' not found"
**When it occurs**: Referencing a schema that doesn't exist
**Common causes**:
- Schema name typo
- Schema was dropped
- Wrong database/schema

**Recovery**:
- Verify schema name
- Create schema if needed
- Check current schema

## Dependency Errors (4500-4599)

Errors related to object dependencies and circular references.

### Error 4500: CircularDependency
**SQLSTATE**: 2BP09
**Message**: "Circular dependency detected: {objects}"
**When it occurs**: Circular references in FOREIGN KEY constraints or other dependencies
**Common causes**:
- Table A references Table B, which references Table A
- Self-referential constraints without proper ordering

**Example**:
```sql
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    referred_by INTEGER REFERENCES users(user_id)
);

-- Create circular reference scenario
-- Table A -> Table B -> Table A
```

**Recovery**:
- Review constraint definitions
- Add `ON DELETE CASCADE` or `ON DELETE SET NULL` as appropriate
- Defer constraints in transactions if needed

---

### Error 4501: DependencyDepthExceeded
**SQLSTATE**: 2BP10
**Message**: "Maximum dependency depth exceeded"
**When it occurs**: Foreign key dependency chain is too deep
**Common causes**:
- Complex interconnected schema with many levels
- Performance consideration limits

**Recovery**:
- Simplify schema design
- Consider denormalization
- Review foreign key necessity

## Transaction Errors (4600-4699)

Errors related to DDL transaction handling and concurrency.

### Error 4600: DdlTransactionFailed
**SQLSTATE**: 40000
**Message**: "DDL transaction failed: {reason}"
**When it occurs**: DDL operation failed within a transaction
**Common causes**:
- Constraint violation preventing operation
- Disk space issues
- System errors

**Example**:
```sql
BEGIN TRANSACTION;
ALTER TABLE users ADD COLUMN email VARCHAR(255) UNIQUE;
-- If operation fails here, entire transaction fails
COMMIT;
-- Error: DDL transaction failed
```

**Recovery**:
- Review transaction log for details
- Fix underlying issue
- Retry transaction

---

### Error 4601: ConcurrentDdlConflict
**SQLSTATE**: 40001
**Message**: "Concurrent DDL conflict detected"
**When it occurs**: Two DDL operations conflict (e.g., schema modifications)
**Common causes**:
- Concurrent ALTER TABLE operations
- Race condition in schema update

**Recovery**:
- Retry operation
- Serialize DDL operations
- Use application-level locking

---

### Error 4602: DdlTimeout
**SQLSTATE**: 57014
**Message**: "DDL operation timeout"
**When it occurs**: DDL operation takes too long and exceeds timeout
**Common causes**:
- Large table modifications
- Lock wait timeout
- Constraint validation on huge datasets

**Recovery**:
- Increase statement timeout
- Run during low-traffic period
- Consider batch processing
- Check for table locks

```sql
-- Increase timeout for specific operation
SET statement_timeout = 120000;  -- 2 minutes
ALTER TABLE large_table ADD COLUMN new_col INTEGER;
```

## Error Handling Best Practices

### 1. Use Conditional Operations

Always use `IF EXISTS` and `IF NOT EXISTS` to make operations idempotent:

```sql
-- Good: Won't fail if table doesn't exist
DROP TABLE IF EXISTS users;

-- Good: Won't fail if table exists
CREATE TABLE IF NOT EXISTS users (
    user_id INTEGER PRIMARY KEY
);

-- Good: Won't fail if constraint exists
ALTER TABLE users DROP CONSTRAINT IF EXISTS uk_email;
```

### 2. Check Dependencies First

Before dropping objects, verify dependencies:

```sql
-- Verify what depends on a table before dropping
-- SELECT * FROM pg_dependent WHERE depended_on_by = 'table_name';

-- Safe drop sequence: dependencies first, then table
DROP TABLE orders;  -- Depends on customers
DROP TABLE customers;  -- Parent table
```

### 3. Use Transactions for Related Changes

Group related DDL operations in transactions:

```sql
BEGIN TRANSACTION;
ALTER TABLE users ADD COLUMN department_id INTEGER;
ALTER TABLE users ADD CONSTRAINT fk_dept
    FOREIGN KEY (department_id) REFERENCES departments(id);
COMMIT;  -- All-or-nothing execution
```

### 4. Plan Constraint Naming

Use consistent naming for constraints to avoid conflicts:

```sql
-- Pattern: constraint_type_table_[columns]
ALTER TABLE users ADD CONSTRAINT pk_users PRIMARY KEY (user_id);
ALTER TABLE users ADD CONSTRAINT uk_users_email UNIQUE (email);
ALTER TABLE orders ADD CONSTRAINT fk_orders_customers FOREIGN KEY (customer_id) REFERENCES customers(customer_id);
ALTER TABLE orders ADD CONSTRAINT ck_orders_amount CHECK (amount > 0);
```

### 5. Handle Errors in Application Code

Applications should handle DDL errors gracefully:

```rust
// Pseudocode: Error handling in application
match execute_ddl(sql) {
    Ok(_) => println!("DDL succeeded"),
    Err(DdlError::TableAlreadyExists(_)) => {
        // Already exists, not an error in this context
        println!("Table already exists, continuing");
    },
    Err(DdlError::ConstraintViolation(msg)) => {
        // Constraint violation, needs data fix
        eprintln!("Fix data: {}", msg);
    },
    Err(e) => {
        eprintln!("Unexpected error: {}", e);
    }
}
```

## Error Message Structure

All DDL errors include:

1. **Error Code**: 4000-4699 range
2. **SQLSTATE**: PostgreSQL-compatible code
3. **Message**: Human-readable description
4. **Context**: Table, column, or constraint involved
5. **Suggestion**: Recommended recovery action

Example error response:

```json
{
    "error_code": 4203,
    "sqlstate": "42601",
    "message": "Invalid constraint definition: Circular foreign key reference",
    "context": {
        "operation": "CREATE_CONSTRAINT",
        "object_type": "CONSTRAINT",
        "object_name": "fk_orders_users",
        "table_name": "orders"
    },
    "suggestion": "Review foreign key references for circular dependencies"
}
```

## Getting Help

For more detailed information:
- See SCHEMA_MIGRATION_GUIDE.md for migration-specific errors
- See DEVELOPER_GUIDE.md for implementation details
- Check application logs for detailed error context
- Review constraint definitions and table schemas

