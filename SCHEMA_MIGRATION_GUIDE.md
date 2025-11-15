# Schema Migration Guide

This guide explains how to safely modify database schemas using ALTER TABLE operations in RustgreSQL.

## Overview

Schema migrations allow you to evolve your database structure over time as application requirements change. RustgreSQL provides comprehensive ALTER TABLE support for adding columns, dropping columns, modifying constraints, and renaming tables and columns.

**Key Principle**: Always plan schema changes carefully and test in non-production environments first. Some operations are reversible; others require careful consideration.

## ALTER TABLE Operations

### Adding Columns

Adding a new column to an existing table is one of the safest schema operations.

#### Basic Column Addition

```sql
ALTER TABLE users ADD COLUMN phone VARCHAR(20);
```

#### Column Addition with Default Value

When adding a column to a table with existing data, provide a default value to populate existing rows:

```sql
ALTER TABLE users ADD COLUMN created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP;

-- All existing rows will get the default value for created_at
```

#### Column Addition with Constraints

```sql
-- Add a NOT NULL column (requires default for existing data)
ALTER TABLE users ADD COLUMN status VARCHAR(20) NOT NULL DEFAULT 'active';

-- Add a UNIQUE column
ALTER TABLE users ADD COLUMN username VARCHAR(255) UNIQUE;

-- Add a CHECK constraint
ALTER TABLE orders ADD COLUMN quantity INTEGER CHECK (quantity > 0);
```

**Best Practice**: For nullable columns without defaults, add the column as nullable first, then populate data, then add constraints.

### Dropping Columns

Dropping columns removes them permanently from the table structure and deletes all data in that column.

```sql
-- Drop a single column
ALTER TABLE users DROP COLUMN phone;

-- Drop multiple columns (sequential operations)
ALTER TABLE users DROP COLUMN phone;
ALTER TABLE users DROP COLUMN fax;
```

**Warning**: This operation is irreversible. Ensure you have backups before dropping columns in production.

**Dependency Checking**: RustgreSQL prevents dropping columns that are:
- Part of PRIMARY KEY constraints
- Referenced by FOREIGN KEY constraints
- Used in indexes
- Used in CHECK constraints

### Adding Constraints

Adding constraints to existing tables validates existing data against the constraint definition.

#### Adding PRIMARY KEY

```sql
-- Add PRIMARY KEY to a column
ALTER TABLE products ADD CONSTRAINT pk_products PRIMARY KEY (product_id);
```

**Requirement**: All values in the column(s) must be unique and non-null.

#### Adding UNIQUE Constraints

```sql
-- Add UNIQUE constraint to a single column
ALTER TABLE users ADD CONSTRAINT uk_email UNIQUE (email);

-- Add UNIQUE constraint to multiple columns (composite unique)
ALTER TABLE order_items ADD CONSTRAINT uk_order_product UNIQUE (order_id, product_id);
```

**Validation**: RustgreSQL scans all existing data to ensure uniqueness.

#### Adding FOREIGN KEY Constraints

```sql
-- Add FOREIGN KEY constraint
ALTER TABLE orders ADD CONSTRAINT fk_customer FOREIGN KEY (customer_id) REFERENCES customers(customer_id);
```

**Validation**: All values in the column(s) must exist in the referenced table.

**Referential Integrity**: Once the constraint is added, future INSERT/UPDATE operations must maintain referential integrity.

#### Adding CHECK Constraints

```sql
-- Add CHECK constraint with simple condition
ALTER TABLE employees ADD CONSTRAINT ck_salary CHECK (salary > 0);

-- Add CHECK constraint with complex condition
ALTER TABLE orders ADD CONSTRAINT ck_dates CHECK (order_date <= delivery_date);
```

**Validation**: RustgreSQL evaluates the CHECK condition against all existing data.

### Dropping Constraints

Remove constraints from a table:

```sql
-- Drop a constraint by name
ALTER TABLE users DROP CONSTRAINT uk_email;

-- Drop PRIMARY KEY (uses default constraint naming)
ALTER TABLE users DROP CONSTRAINT pk_users;
```

**Impact**: Dropping a constraint relaxes validation rules for future operations.

### Renaming

Rename columns, tables, and other objects:

```sql
-- Rename a column
ALTER TABLE users RENAME COLUMN name TO full_name;

-- Rename a table
ALTER TABLE users RENAME TO customer_accounts;
```

**Dependency Impact**: Applications using these names need to be updated.

## Safe Schema Evolution Strategy

### Phase 1: Preparation

Before making schema changes:

1. **Backup**: Create a full database backup
2. **Plan**: Document all changes and their order
3. **Test**: Execute migrations in a test environment
4. **Review**: Get approval from team members

### Phase 2: Additive Changes (Safest)

Additive changes don't break existing applications:

```sql
-- Add nullable column (backward compatible)
ALTER TABLE users ADD COLUMN middle_name VARCHAR(100);

-- Add column with default (backward compatible)
ALTER TABLE products ADD COLUMN discontinued BOOLEAN DEFAULT false;

-- Add new optional constraints
ALTER TABLE orders ADD CONSTRAINT ck_amount CHECK (amount >= 0);
```

**Benefits**:
- Existing queries continue to work
- No data loss
- Easy to rollback if needed

### Phase 3: Subtractive Changes (High Risk)

Subtractive changes require careful planning:

```sql
-- Remove unused column after verification
ALTER TABLE users DROP COLUMN legacy_field;

-- Drop constraint to relax validation
ALTER TABLE orders DROP CONSTRAINT ck_positive_quantity;
```

**Risks**:
- Permanent data loss
- Applications expecting the column will fail
- Difficult to rollback

### Phase 4: Verification

After migration:

1. **Validate**: Run test suite against new schema
2. **Check**: Query systems using affected tables
3. **Monitor**: Watch for errors in application logs
4. **Verify**: Confirm data integrity

## Handling Default Values

Default values are crucial when adding columns to tables with existing data.

### CURRENT_TIMESTAMP

```sql
-- Add created_at column with current time default
ALTER TABLE users ADD COLUMN created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP;
```

Each existing row will get the timestamp when this statement executed.

### Static Values

```sql
-- Add status column with static default
ALTER TABLE users ADD COLUMN status VARCHAR(20) DEFAULT 'active';

-- Add numeric default
ALTER TABLE products ADD COLUMN stock_count INTEGER DEFAULT 0;
```

### NULL Values (No Default)

```sql
-- Add nullable column (no default)
ALTER TABLE users ADD COLUMN preferences TEXT;

-- Existing rows get NULL, new rows can be NULL unless specified
```

### Data Population After Addition

For complex defaults, add the column as nullable, then populate:

```sql
-- Step 1: Add column as nullable
ALTER TABLE orders ADD COLUMN profit_margin NUMERIC(5, 2);

-- Step 2: Populate using UPDATE
-- UPDATE orders SET profit_margin = CALCULATE_MARGIN(price, cost);

-- Step 3: Add constraint if needed
ALTER TABLE orders ADD CONSTRAINT ck_margin CHECK (profit_margin >= 0);
```

## Transaction Safety

All ALTER TABLE operations in RustgreSQL are transactional:

```sql
BEGIN TRANSACTION;

ALTER TABLE users ADD COLUMN email VARCHAR(255) UNIQUE;
ALTER TABLE users ADD CONSTRAINT ck_email CHECK (email LIKE '%@%.%');

-- If any operation fails, all changes rollback
COMMIT;
```

**Key Properties**:
- **Atomicity**: All-or-nothing execution
- **Isolation**: Other transactions see consistent state
- **Durability**: Committed changes are safe after completion

## Rollback Scenarios

### Rollback During Transaction

If an ALTER operation fails within a transaction, rollback all changes:

```sql
BEGIN TRANSACTION;

ALTER TABLE products ADD COLUMN sku VARCHAR(50) UNIQUE;
-- This might fail if SKU values aren't unique
ALTER TABLE products ADD CONSTRAINT ck_sku CHECK (LENGTH(sku) > 0);

-- If second operation fails:
ROLLBACK;  -- Both operations are undone
```

### Rollback After Commit (Reverse Operations)

If a migration causes problems after commit, use reverse migrations:

```sql
-- Original migration
ALTER TABLE users DROP COLUMN legacy_field;

-- Reverse migration (requires backup or manual recreation)
ALTER TABLE users ADD COLUMN legacy_field TEXT;
```

## Constraint Naming Strategy

Use consistent naming conventions for constraints:

```sql
-- Primary Key: pk_<table_name>
ALTER TABLE users ADD CONSTRAINT pk_users PRIMARY KEY (user_id);

-- Unique: uk_<table_name>_<columns>
ALTER TABLE users ADD CONSTRAINT uk_users_email UNIQUE (email);

-- Foreign Key: fk_<table_name>_<referenced_table>
ALTER TABLE orders ADD CONSTRAINT fk_orders_customers FOREIGN KEY (customer_id) REFERENCES customers(customer_id);

-- Check: ck_<table_name>_<purpose>
ALTER TABLE orders ADD CONSTRAINT ck_orders_amount CHECK (amount > 0);
```

**Benefits**:
- Easier to identify constraints in error messages
- Simpler to drop specific constraints
- Better documentation in schema

## Common Migration Patterns

### Renaming a Table (with Application Updates)

```sql
-- Step 1: Rename table
ALTER TABLE customers RENAME TO customer_accounts;

-- Step 2: Update application code to use new table name
-- Step 3: Update view definitions, stored procedures, etc.
```

### Adding a New Required Column

```sql
-- Step 1: Add column as nullable with default
ALTER TABLE users ADD COLUMN department_id INTEGER DEFAULT 1;

-- Step 2: Update application to require this field for new rows
-- Step 3: Populate existing rows with proper values (if needed)
-- UPDATE users SET department_id = <appropriate_value>;

-- Step 4: Add NOT NULL constraint
ALTER TABLE users ADD CONSTRAINT nn_department CHECK (department_id IS NOT NULL);
```

### Splitting a Column into Multiple Columns

```sql
-- Step 1: Add new columns
ALTER TABLE users ADD COLUMN first_name VARCHAR(100);
ALTER TABLE users ADD COLUMN last_name VARCHAR(100);

-- Step 2: Populate new columns from existing data
-- UPDATE users SET first_name = SPLIT_PART(full_name, ' ', 1);
-- UPDATE users SET last_name = SPLIT_PART(full_name, ' ', 2);

-- Step 3: Drop original column
ALTER TABLE users DROP COLUMN full_name;
```

### Merging Multiple Columns into One

```sql
-- Step 1: Add new combined column
ALTER TABLE users ADD COLUMN full_name VARCHAR(200);

-- Step 2: Populate from existing columns
-- UPDATE users SET full_name = CONCAT(first_name, ' ', last_name);

-- Step 3: Drop original columns
ALTER TABLE users DROP COLUMN first_name;
ALTER TABLE users DROP COLUMN last_name;
```

### Adding Audit Columns

```sql
-- Add audit columns for tracking changes
ALTER TABLE users ADD COLUMN created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP;
ALTER TABLE users ADD COLUMN updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP;
ALTER TABLE users ADD COLUMN created_by VARCHAR(100) DEFAULT 'system';
ALTER TABLE users ADD COLUMN updated_by VARCHAR(100) DEFAULT 'system';
```

## Performance Considerations

### Large Table Migrations

For tables with millions of rows:

1. **Plan carefully**: Schema changes lock the table during modification
2. **Schedule during low-traffic**: Execute during maintenance window
3. **Monitor**: Watch database performance during migration
4. **Test**: Run migration on production-size test data first

### Index Management During Migrations

```sql
-- Adding constraint may automatically create indexes
ALTER TABLE users ADD CONSTRAINT uk_email UNIQUE (email);  -- Creates index

-- Explicit index creation for performance
CREATE INDEX idx_users_created_at ON users(created_at);

-- Drop unused indexes to improve migration performance
-- DROP INDEX idx_old_field;
```

## Schema Evolution Example

Complete example of evolving a user management schema:

```sql
-- Version 1.0: Initial schema
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL
);

-- Version 1.1: Add user profile fields
ALTER TABLE users ADD COLUMN first_name VARCHAR(100);
ALTER TABLE users ADD COLUMN last_name VARCHAR(100);
ALTER TABLE users ADD COLUMN profile_picture_url VARCHAR(500);

-- Version 1.2: Add account management
ALTER TABLE users ADD COLUMN is_active BOOLEAN DEFAULT true;
ALTER TABLE users ADD COLUMN last_login TIMESTAMP;

-- Version 1.3: Add audit fields
ALTER TABLE users ADD COLUMN created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP;
ALTER TABLE users ADD COLUMN updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP;

-- Version 1.4: Enforce data quality
ALTER TABLE users ADD CONSTRAINT ck_email_format CHECK (email LIKE '%@%');
ALTER TABLE users ADD CONSTRAINT ck_name_not_empty CHECK (LENGTH(first_name) > 0);

-- Version 1.5: Add relationships
CREATE TABLE user_roles (
    user_id INTEGER REFERENCES users(user_id),
    role_name VARCHAR(50),
    PRIMARY KEY (user_id, role_name)
);

ALTER TABLE users ADD CONSTRAINT fk_users_roles FOREIGN KEY (user_id) REFERENCES user_roles(user_id);
```

## Tools and Utilities

### Verification Queries

Check schema changes before and after:

```sql
-- List all columns in a table
-- SELECT * FROM information_schema.columns WHERE table_name = 'users';

-- Check constraints on a table
-- SELECT * FROM information_schema.table_constraints WHERE table_name = 'users';

-- Verify index usage
-- SELECT * FROM pg_indexes WHERE tablename = 'users';
```

### Backup Before Major Changes

```bash
# Create backup before migration
cp rustgresql.db rustgresql.db.backup

# Run migration
cargo run

# Restore if needed
cp rustgresql.db.backup rustgresql.db
```

## Summary

Safe schema migrations require:

1. **Planning**: Document all changes before executing
2. **Testing**: Validate in non-production first
3. **Backup**: Always have a recovery option
4. **Transaction Safety**: Use transactions for related changes
5. **Verification**: Test thoroughly after migration
6. **Rollback Plan**: Know how to undo changes if needed

For more information on DDL implementation details, see the DEVELOPER_GUIDE.md section on DDL Implementation Architecture.
