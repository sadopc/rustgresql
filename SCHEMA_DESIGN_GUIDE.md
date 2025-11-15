# Schema Design Guide

This guide provides best practices and design patterns for creating robust, maintainable, and performant database schemas in RustgreSQL. The patterns and recommendations are based on the complete DDL implementation and are compatible with PostgreSQL standards.

## Table of Contents

1. [Design Principles](#design-principles)
2. [Naming Conventions](#naming-conventions)
3. [Constraint Design](#constraint-design)
4. [Primary Key Selection](#primary-key-selection)
5. [Foreign Key Design](#foreign-key-design)
6. [Index Strategy](#index-strategy)
7. [Data Type Selection](#data-type-selection)
8. [Performance Patterns](#performance-patterns)
9. [Anti-Patterns](#anti-patterns)
10. [Normalization Guidelines](#normalization-guidelines)
11. [Schema Versioning](#schema-versioning)
12. [Real-World Patterns](#real-world-patterns)

## Design Principles

### 1. Clarity and Readability

Design schemas that are easy to understand:

```sql
-- GOOD: Clear, descriptive names
CREATE TABLE customer_orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER NOT NULL,
    order_date DATE NOT NULL,
    total_amount NUMERIC(10, 2),
    FOREIGN KEY (customer_id) REFERENCES customers(customer_id)
);

-- AVOID: Ambiguous or cryptic names
CREATE TABLE t1 (
    id INTEGER PRIMARY KEY,
    cid INTEGER NOT NULL,
    odt DATE NOT NULL,
    amt NUMERIC(10, 2)
);
```

**Benefits**:
- Easier for new team members to understand
- Self-documenting code
- Fewer naming bugs
- Better collaboration

### 2. Consistency

Apply consistent patterns throughout your schema:

```sql
-- GOOD: Consistent naming for IDs
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    email VARCHAR(255)
);

CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    user_id INTEGER,
    FOREIGN KEY (user_id) REFERENCES users(user_id)
);

CREATE TABLE payments (
    payment_id INTEGER PRIMARY KEY,
    order_id INTEGER,
    FOREIGN KEY (order_id) REFERENCES orders(order_id)
);

-- AVOID: Inconsistent ID naming
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY
);

CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    uid INTEGER  -- Different naming!
);
```

### 3. Data Integrity First

Use constraints to enforce business rules:

```sql
-- GOOD: Constraints enforce business logic
CREATE TABLE products (
    product_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    price NUMERIC(10, 2) NOT NULL,
    stock_quantity INTEGER NOT NULL,
    CHECK (price > 0),
    CHECK (stock_quantity >= 0)
);

-- AVOID: Relying on application logic alone
CREATE TABLE products (
    product_id INTEGER PRIMARY KEY,
    name VARCHAR(255),
    price NUMERIC(10, 2),
    stock_quantity INTEGER
    -- No constraints!
);
```

### 4. Flexibility for Evolution

Design schemas that can evolve:

```sql
-- GOOD: Add columns with defaults (backward compatible)
ALTER TABLE users ADD COLUMN verified BOOLEAN DEFAULT false;
ALTER TABLE users ADD COLUMN verification_date TIMESTAMP;

-- AVOID: Making schema changes that break existing code
ALTER TABLE users DROP COLUMN created_at;  -- Breaks existing queries
ALTER TABLE users ALTER COLUMN email TYPE BIGINT;  -- Type change
```

## Naming Conventions

### Table Naming

**Recommendations**:
- Use plural names: `users`, `products`, `orders`
- Use snake_case: `customer_orders`, `order_items`
- Be descriptive: `inventory_transactions` vs `trans`
- Avoid reserved keywords: Use `user_accounts` instead of `user`

```sql
-- GOOD naming examples
CREATE TABLE customers (customer_id INTEGER PRIMARY KEY);
CREATE TABLE customer_addresses (address_id INTEGER PRIMARY KEY);
CREATE TABLE customer_orders (order_id INTEGER PRIMARY KEY);
CREATE TABLE order_items (order_item_id INTEGER PRIMARY KEY);
CREATE TABLE product_inventory (inventory_id INTEGER PRIMARY KEY);
```

### Column Naming

**Recommendations**:
- Use lowercase with underscores: `first_name`, `created_at`
- Include data type hints in name when helpful: `email_address`, `phone_number`
- Use consistent ID naming: `table_id` for foreign keys
- Use timestamp suffix for dates: `created_at`, `updated_at`, `deleted_at`

```sql
-- GOOD naming examples
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    first_name VARCHAR(100),
    last_name VARCHAR(100),
    email_address VARCHAR(255) UNIQUE,
    phone_number VARCHAR(20),
    date_of_birth DATE,
    created_at TIMESTAMP,
    updated_at TIMESTAMP
);
```

### Constraint Naming

Use consistent patterns for constraint names:

```sql
-- Pattern: constraint_type_table_[columns]

-- Primary key: pk_<table_name>
CONSTRAINT pk_users PRIMARY KEY (user_id)

-- Unique: uk_<table_name>_<columns>
CONSTRAINT uk_users_email UNIQUE (email)
CONSTRAINT uk_users_username UNIQUE (username)

-- Foreign key: fk_<table_name>_<referenced_table>
CONSTRAINT fk_orders_customers FOREIGN KEY (customer_id) REFERENCES customers(customer_id)
CONSTRAINT fk_order_items_products FOREIGN KEY (product_id) REFERENCES products(product_id)

-- Check: ck_<table_name>_<condition>
CONSTRAINT ck_products_price CHECK (price > 0)
CONSTRAINT ck_users_age CHECK (age >= 18)

-- Example table with named constraints
CREATE TABLE orders (
    order_id INTEGER,
    customer_id INTEGER,
    total_amount NUMERIC(10, 2),
    status VARCHAR(20),

    CONSTRAINT pk_orders PRIMARY KEY (order_id),
    CONSTRAINT fk_orders_customers FOREIGN KEY (customer_id) REFERENCES customers(customer_id),
    CONSTRAINT ck_orders_amount CHECK (total_amount >= 0),
    CONSTRAINT ck_orders_status CHECK (status IN ('draft', 'submitted', 'processed', 'cancelled'))
);
```

### Index Naming

**Recommendations**:
- Use prefix `idx_` for regular indexes
- Include table name and column(s): `idx_users_email`
- Composite indexes: `idx_orders_customer_date`

```sql
-- Good index naming
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_orders_customer_date ON orders(customer_id, order_date);
CREATE INDEX idx_products_category ON products(category_id);
```

## Constraint Design

### Column vs Table Constraints

Choose the appropriate constraint level:

```sql
-- GOOD: Column-level constraints for simple cases
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    username VARCHAR(50) NOT NULL,
    birth_year INTEGER CHECK (birth_year > 1900)
);

-- GOOD: Table-level constraints for complex cases
CREATE TABLE order_items (
    order_id INTEGER,
    item_id INTEGER,
    quantity INTEGER,
    unit_price NUMERIC(10, 2),

    -- Composite PRIMARY KEY
    PRIMARY KEY (order_id, item_id),

    -- CHECK with multiple columns
    CHECK (quantity > 0 AND unit_price > 0)
);

-- BEST: Named table-level constraints for clarity
CREATE TABLE invoices (
    invoice_id INTEGER,
    customer_id INTEGER,
    amount NUMERIC(10, 2),
    status VARCHAR(20),

    CONSTRAINT pk_invoices PRIMARY KEY (invoice_id),
    CONSTRAINT fk_customer FOREIGN KEY (customer_id) REFERENCES customers(customer_id),
    CONSTRAINT ck_amount CHECK (amount > 0),
    CONSTRAINT ck_status CHECK (status IN ('draft', 'sent', 'paid'))
);
```

**When to use each**:
- Column-level: Single column constraints (PRIMARY KEY, UNIQUE, NOT NULL, DEFAULT, CHECK)
- Table-level: Composite constraints, foreign keys, complex conditions, explicit naming

### DEFAULT Values

Use defaults strategically:

```sql
-- GOOD: Sensible defaults
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    email VARCHAR(255) NOT NULL,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    status VARCHAR(20) DEFAULT 'active'
);

-- GOOD: NULL for optional fields (no default needed)
CREATE TABLE profiles (
    profile_id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
    bio TEXT,  -- NULL if not provided
    website_url VARCHAR(255),  -- NULL if not provided
    phone_number VARCHAR(20)  -- NULL if not provided
);

-- AVOID: Misleading defaults
CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    amount NUMERIC(10, 2) DEFAULT 0,  -- Should NOT default to 0
    customer_id INTEGER DEFAULT 1  -- Which customer?
);
```

### NOT NULL Constraints

Apply NOT NULL strategically:

```sql
-- GOOD: NOT NULL for required business data
CREATE TABLE employees (
    employee_id INTEGER PRIMARY KEY,
    first_name VARCHAR(100) NOT NULL,  -- Always required
    last_name VARCHAR(100) NOT NULL,  -- Always required
    email VARCHAR(255) NOT NULL UNIQUE,  -- Always unique
    hire_date DATE NOT NULL,  -- Always required
    phone VARCHAR(20)  -- Optional
);

-- AVOID: Excessive NOT NULL constraints
CREATE TABLE events (
    event_id INTEGER PRIMARY KEY NOT NULL,  -- Redundant (PK implies NOT NULL)
    event_name VARCHAR(255) NOT NULL,  -- OK
    description TEXT NOT NULL,  -- Too strict if description is often empty
    notes TEXT NOT NULL  -- Too strict
);
```

## Primary Key Selection

### Surrogate Keys (Recommended)

Use auto-incrementing integers as primary keys:

```sql
-- GOOD: Surrogate key
CREATE TABLE customers (
    customer_id INTEGER PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL
);

-- BENEFITS:
-- 1. Small, efficient (4-8 bytes)
-- 2. No business logic entanglement
-- 3. Easy to reference in foreign keys
-- 4. Stable even if email changes
```

### Natural Keys

Use when they exist and are stable:

```sql
-- GOOD: Natural key when appropriate
CREATE TABLE countries (
    country_code CHAR(2) PRIMARY KEY,  -- ISO 3166-1 alpha-2
    country_name VARCHAR(100) NOT NULL
);

-- Composite natural key
CREATE TABLE currency_rates (
    from_currency CHAR(3),
    to_currency CHAR(3),
    rate_date DATE,
    rate NUMERIC(10, 6),

    PRIMARY KEY (from_currency, to_currency, rate_date)
);

-- AVOID: Natural keys that change frequently
CREATE TABLE users (
    email VARCHAR(255) PRIMARY KEY,  -- Email can change!
    name VARCHAR(255)
);

-- SOLUTION: Use surrogate key with unique index on email
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255)
);
```

### Composite Primary Keys

Use for junction tables and natural multi-column scenarios:

```sql
-- GOOD: Composite PK for junction table
CREATE TABLE student_courses (
    student_id INTEGER,
    course_id INTEGER,
    PRIMARY KEY (student_id, course_id),
    FOREIGN KEY (student_id) REFERENCES students(student_id),
    FOREIGN KEY (course_id) REFERENCES courses(course_id)
);

-- GOOD: Composite PK for time-series data
CREATE TABLE daily_metrics (
    metric_id INTEGER,
    date DATE,
    value NUMERIC(10, 2),
    PRIMARY KEY (metric_id, date)
);

-- AVOID: Overly complex composite keys
CREATE TABLE complex_key (
    col1 VARCHAR(50),
    col2 VARCHAR(50),
    col3 VARCHAR(50),
    col4 VARCHAR(50),
    PRIMARY KEY (col1, col2, col3, col4)
    -- Too complex, use surrogate key instead
);
```

## Foreign Key Design

### Basic Foreign Key Pattern

```sql
-- GOOD: Clear foreign key relationship
CREATE TABLE departments (
    department_id INTEGER PRIMARY KEY,
    name VARCHAR(100) NOT NULL
);

CREATE TABLE employees (
    employee_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    department_id INTEGER NOT NULL,
    FOREIGN KEY (department_id) REFERENCES departments(department_id)
);
```

### Optional Foreign Keys

Allow NULL for optional relationships:

```sql
-- GOOD: Optional supervisor
CREATE TABLE employees (
    employee_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    supervisor_id INTEGER,  -- NULL if no supervisor
    FOREIGN KEY (supervisor_id) REFERENCES employees(employee_id)
);

-- Insert employees with and without supervisors
INSERT INTO employees VALUES (1, 'Alice', NULL);  -- CEO
INSERT INTO employees VALUES (2, 'Bob', 1);  -- Reports to Alice
INSERT INTO employees VALUES (3, 'Charlie', 1);  -- Also reports to Alice
```

### Referential Integrity Options

```sql
-- GOOD: RESTRICT (default) - prevent deletion if referenced
CREATE TABLE customers (
    customer_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL
);

CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER NOT NULL,
    FOREIGN KEY (customer_id) REFERENCES customers(customer_id)
    -- Implicit RESTRICT
);

-- Try to delete customer with orders - ERROR

-- GOOD: CASCADE for dependent data
CREATE TABLE order_items (
    order_item_id INTEGER PRIMARY KEY,
    order_id INTEGER NOT NULL,
    product_id INTEGER NOT NULL,
    FOREIGN KEY (order_id) REFERENCES orders(order_id)
    -- If order is deleted, cascade delete items
);
```

### Avoid Circular Dependencies

```sql
-- PROBLEM: Circular foreign key
-- CREATE TABLE table_a (
--     id INTEGER PRIMARY KEY,
--     table_b_id INTEGER REFERENCES table_b(id)
-- );
-- CREATE TABLE table_b (
--     id INTEGER PRIMARY KEY,
--     table_a_id INTEGER REFERENCES table_a(id)
-- );

-- SOLUTION 1: Remove one of the relationships
CREATE TABLE table_a (
    id INTEGER PRIMARY KEY,
    name VARCHAR(255)
);

CREATE TABLE table_b (
    id INTEGER PRIMARY KEY,
    table_a_id INTEGER,
    FOREIGN KEY (table_a_id) REFERENCES table_a(id)
);

-- SOLUTION 2: Use a junction table
CREATE TABLE table_a (
    id INTEGER PRIMARY KEY,
    name VARCHAR(255)
);

CREATE TABLE table_b (
    id INTEGER PRIMARY KEY,
    name VARCHAR(255)
);

CREATE TABLE a_b_relationship (
    table_a_id INTEGER,
    table_b_id INTEGER,
    PRIMARY KEY (table_a_id, table_b_id),
    FOREIGN KEY (table_a_id) REFERENCES table_a(id),
    FOREIGN KEY (table_b_id) REFERENCES table_b(id)
);
```

## Index Strategy

### When to Create Indexes

```sql
-- GOOD: Index foreign keys
CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER NOT NULL,
    FOREIGN KEY (customer_id) REFERENCES customers(customer_id)
);
CREATE INDEX idx_orders_customer ON orders(customer_id);

-- GOOD: Index frequently searched columns
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    email VARCHAR(255) UNIQUE,  -- Auto-indexed
    username VARCHAR(50) NOT NULL
);
CREATE INDEX idx_users_username ON users(username);

-- GOOD: Composite indexes for multi-column queries
CREATE INDEX idx_orders_customer_date ON orders(customer_id, order_date);

-- GOOD: Columns used in WHERE clauses often
CREATE TABLE products (
    product_id INTEGER PRIMARY KEY,
    category_id INTEGER,
    is_active BOOLEAN
);
CREATE INDEX idx_products_active_category ON products(is_active, category_id);
```

### When NOT to Create Indexes

```sql
-- AVOID: Indexing low-cardinality columns
CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    status VARCHAR(20),  -- Only 5 possible values
    -- Don't index: BOOLEAN or very few distinct values
    is_active BOOLEAN
);

-- AVOID: Redundant indexes
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,  -- Already indexed
    email VARCHAR(255) UNIQUE  -- Unique creates index
);
-- Don't create another index on email

-- AVOID: Too many indexes
CREATE TABLE huge_table (
    id INTEGER PRIMARY KEY,
    col1 VARCHAR(255),
    col2 INTEGER,
    col3 VARCHAR(255),
    col4 INTEGER,
    col5 VARCHAR(255)
);
-- Don't index every column: maintenance overhead > benefit
```

## Data Type Selection

### Use Appropriate Types

```sql
-- GOOD: Right types for the data
CREATE TABLE users (
    user_id INTEGER,  -- Whole numbers
    email VARCHAR(255),  -- Strings with limit
    description TEXT,  -- Long text
    birth_date DATE,  -- Just date
    created_at TIMESTAMP,  -- Date and time
    is_active BOOLEAN,  -- True/false
    rating NUMERIC(3, 1)  -- 99.9 range
);

-- AVOID: Wrong types
CREATE TABLE users (
    user_id VARCHAR(20),  -- Should be INTEGER
    email TEXT,  -- Should be VARCHAR
    birth_date TEXT,  -- Should be DATE
    is_active VARCHAR(1),  -- Should be BOOLEAN
    rating NUMERIC(10, 2)  -- Overkill for 0-10 rating
);
```

### String Types

```sql
-- GOOD: Use appropriate string types
CREATE TABLE examples (
    country_code CHAR(2),  -- Fixed length (ISO codes)
    postal_code CHAR(5),  -- Fixed length (US zip)
    username VARCHAR(50),  -- Variable, with reasonable limit
    bio TEXT,  -- Unlimited length
    email VARCHAR(255)  -- Email addresses
);
```

### Numeric Precision

```sql
-- GOOD: Appropriate precision
CREATE TABLE products (
    price NUMERIC(10, 2),  -- Up to 99,999,999.99
    cost NUMERIC(10, 2),
    discount_percent NUMERIC(5, 2)  -- Up to 999.99
);

-- GOOD: No decimal places when not needed
CREATE TABLE inventory (
    quantity INTEGER,  -- Whole items only
    warehouse_id SMALLINT,  -- Small number (0-32767)
    stock_count INTEGER  -- Whole count
);
```

## Performance Patterns

### Denormalization for Read Performance

```sql
-- GOOD: Denormalize when reads are much more common than writes
CREATE TABLE order_summary (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER,
    customer_name VARCHAR(255),  -- Denormalized from customers table
    order_date DATE,
    total_items INTEGER,  -- Count of items
    total_amount NUMERIC(10, 2),  -- Calculated total
    FOREIGN KEY (customer_id) REFERENCES customers(customer_id)
);

-- NOTE: Keep denormalized data in sync via triggers or application logic
```

### Partitioning Strategy

```sql
-- For large tables, consider partitioning by date
-- CREATE TABLE events (
--     event_id INTEGER PRIMARY KEY,
--     event_date DATE,
--     data TEXT
-- ) PARTITION BY RANGE (YEAR(event_date));

-- Or by ID ranges
-- CREATE TABLE logs_0_to_1m AS
-- SELECT * FROM logs WHERE id < 1000000;
```

### Archive Tables for Historical Data

```sql
-- Keep active data in main table
CREATE TABLE active_orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER,
    order_date DATE,
    status VARCHAR(20)
);

-- Archive old data
-- CREATE TABLE archived_orders AS
-- SELECT * FROM orders WHERE order_date < DATE '2020-01-01';
```

## Anti-Patterns

### 1. Too Much Normalization

```sql
-- AVOID: Over-normalized (too many joins needed)
CREATE TABLE first_names (
    first_name_id INTEGER PRIMARY KEY,
    first_name VARCHAR(100)
);

CREATE TABLE last_names (
    last_name_id INTEGER PRIMARY KEY,
    last_name VARCHAR(100)
);

CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    first_name_id INTEGER,
    last_name_id INTEGER,
    FOREIGN KEY (first_name_id) REFERENCES first_names(first_name_id),
    FOREIGN KEY (last_name_id) REFERENCES last_names(last_name_id)
);

-- GOOD: Keep it simple
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    first_name VARCHAR(100),
    last_name VARCHAR(100)
);
```

### 2. EAV (Entity-Attribute-Value) Pattern

```sql
-- AVOID: EAV makes queries complex and slow
CREATE TABLE entity_attributes (
    entity_id INTEGER,
    attribute_name VARCHAR(100),
    attribute_value VARCHAR(255)
);

-- GOOD: Use proper columns
CREATE TABLE products (
    product_id INTEGER PRIMARY KEY,
    name VARCHAR(255),
    price NUMERIC(10, 2),
    weight NUMERIC(10, 2),
    color VARCHAR(50)
);
```

### 3. Polymorphic Associations

```sql
-- AVOID: Single FK to multiple table types
-- CREATE TABLE comments (
--     comment_id INTEGER PRIMARY KEY,
--     target_type VARCHAR(50),  -- 'post' or 'user'
--     target_id INTEGER  -- Ambiguous!
-- );

-- GOOD: Separate foreign keys
CREATE TABLE post_comments (
    comment_id INTEGER PRIMARY KEY,
    post_id INTEGER,
    FOREIGN KEY (post_id) REFERENCES posts(post_id)
);

CREATE TABLE user_comments (
    comment_id INTEGER PRIMARY KEY,
    user_id INTEGER,
    FOREIGN KEY (user_id) REFERENCES users(user_id)
);
```

### 4. Missing Timestamps

```sql
-- AVOID: No audit trail
CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER
);

-- GOOD: Include audit timestamps
CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP
);
```

## Normalization Guidelines

### First Normal Form (1NF)

Each column contains atomic (non-repeating) values:

```sql
-- AVOID: Non-atomic values
CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    customer_name VARCHAR(255),
    items_ordered VARCHAR(255)  -- Comma-separated!
);

-- GOOD: Atomic values
CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER
);

CREATE TABLE order_items (
    order_item_id INTEGER PRIMARY KEY,
    order_id INTEGER,
    product_id INTEGER,
    FOREIGN KEY (order_id) REFERENCES orders(order_id),
    FOREIGN KEY (product_id) REFERENCES products(product_id)
);
```

### Second Normal Form (2NF)

Non-key attributes depend on the entire primary key:

```sql
-- GOOD: Proper 2NF design
CREATE TABLE student_courses (
    student_id INTEGER,
    course_id INTEGER,
    semester VARCHAR(20),
    grade CHAR(1),
    PRIMARY KEY (student_id, course_id, semester)
    -- Grade depends on all parts of the key
);
```

### Third Normal Form (3NF)

Non-key attributes don't depend on other non-key attributes:

```sql
-- AVOID: Violates 3NF
CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER,
    customer_name VARCHAR(255),  -- Depends on customer_id, not order_id
    customer_email VARCHAR(255)
);

-- GOOD: Proper 3NF
CREATE TABLE customers (
    customer_id INTEGER PRIMARY KEY,
    name VARCHAR(255),
    email VARCHAR(255)
);

CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER,
    FOREIGN KEY (customer_id) REFERENCES customers(customer_id)
);
```

## Schema Versioning

### Version Tracking

```sql
-- Keep track of schema versions
CREATE TABLE schema_versions (
    version_id INTEGER PRIMARY KEY,
    version_number VARCHAR(20) NOT NULL,
    description TEXT,
    applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO schema_versions VALUES (1, '1.0.0', 'Initial schema');
INSERT INTO schema_versions VALUES (2, '1.1.0', 'Added user profiles');

-- Current query: SELECT MAX(version_number) FROM schema_versions;
```

### Migration Scripts

```sql
-- Version 1.1.0 migration
-- Add user preferences
ALTER TABLE users ADD COLUMN preferences JSON;
ALTER TABLE users ADD COLUMN theme VARCHAR(20) DEFAULT 'light';

-- Version 1.2.0 migration
-- Add audit fields
ALTER TABLE users ADD COLUMN last_login TIMESTAMP;
ALTER TABLE users ADD COLUMN login_count INTEGER DEFAULT 0;
```

## Real-World Patterns

### SaaS Multi-Tenant Schema

```sql
CREATE TABLE organizations (
    organization_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    organization_id INTEGER NOT NULL,
    email VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    role VARCHAR(50),
    FOREIGN KEY (organization_id) REFERENCES organizations(organization_id),
    UNIQUE (organization_id, email)  -- Email unique per org
);

-- All tenant-specific tables include organization_id
CREATE TABLE projects (
    project_id INTEGER PRIMARY KEY,
    organization_id INTEGER NOT NULL,
    name VARCHAR(255) NOT NULL,
    created_by INTEGER,
    FOREIGN KEY (organization_id) REFERENCES organizations(organization_id),
    FOREIGN KEY (created_by) REFERENCES users(user_id)
);
```

### Time-Series Data

```sql
-- Efficient time-series schema
CREATE TABLE metrics (
    metric_id INTEGER,
    timestamp TIMESTAMP,
    value NUMERIC(10, 2),
    dimension VARCHAR(100),

    PRIMARY KEY (metric_id, timestamp)
);

-- Index for time range queries
CREATE INDEX idx_metrics_time ON metrics(timestamp);
```

### Audit Log Pattern

```sql
CREATE TABLE audit_log (
    log_id INTEGER PRIMARY KEY,
    entity_type VARCHAR(100) NOT NULL,
    entity_id INTEGER NOT NULL,
    action VARCHAR(50) NOT NULL,  -- INSERT, UPDATE, DELETE
    old_values TEXT,  -- JSON of previous values
    new_values TEXT,  -- JSON of new values
    changed_by INTEGER,
    changed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_audit_entity ON audit_log(entity_type, entity_id);
CREATE INDEX idx_audit_time ON audit_log(changed_at);
```

## Summary

Good schema design requires:

1. **Clarity** - Use descriptive, consistent names
2. **Integrity** - Use constraints to enforce rules
3. **Normalization** - Eliminate redundancy (but allow some denormalization for performance)
4. **Indexing** - Index strategically for common queries
5. **Evolution** - Design for change and growth
6. **Documentation** - Comment complex designs

For more information:
- See [DDL_EXAMPLES.md](DDL_EXAMPLES.md) for concrete examples
- See [SCHEMA_MIGRATION_GUIDE.md](SCHEMA_MIGRATION_GUIDE.md) for evolution patterns
- See [ERROR_REFERENCE.md](ERROR_REFERENCE.md) for handling constraint violations
- See [DEVELOPER_GUIDE.md](DEVELOPER_GUIDE.md) for implementation details
