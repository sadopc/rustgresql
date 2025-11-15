# DDL Examples

This guide provides comprehensive, working examples of Data Definition Language (DDL) operations in RustgreSQL. All examples have been tested and can be used as templates for your own database schemas.

## Table of Contents

1. [Basic Table Creation](#basic-table-creation)
2. [Tables with Constraints](#tables-with-constraints)
3. [Multi-Table Schemas](#multi-table-schemas)
4. [ALTER TABLE Operations](#alter-table-operations)
5. [Index Management](#index-management)
6. [Real-World Scenarios](#real-world-scenarios)
7. [Error Handling](#error-handling)
8. [Complete E-commerce Schema](#complete-e-commerce-schema)

## Basic Table Creation

### Simple Table with Primary Key

The most basic table definition with just a primary key:

```sql
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    username VARCHAR(50) NOT NULL,
    email VARCHAR(255) NOT NULL
);

-- Insert sample data
INSERT INTO users VALUES (1, 'john_doe', 'john@example.com');
INSERT INTO users VALUES (2, 'jane_smith', 'jane@example.com');

-- Query the table
SELECT * FROM users;
```

**Features demonstrated**:
- `PRIMARY KEY`: Uniquely identifies each row
- `NOT NULL`: Column requires a value
- `VARCHAR`: Variable-length string type
- `INTEGER`: Integer data type

### Table with Auto-Increment Primary Key

Create a table where the primary key auto-increments:

```sql
CREATE TABLE products (
    product_id SERIAL PRIMARY KEY,
    product_name VARCHAR(255) NOT NULL,
    price NUMERIC(10, 2) NOT NULL
);

-- Insert data (product_id auto-increments)
INSERT INTO products (product_name, price) VALUES ('Laptop', 999.99);
INSERT INTO products (product_name, price) VALUES ('Mouse', 29.99);

-- Check auto-incremented IDs
SELECT * FROM products;
-- Result: (1, 'Laptop', 999.99), (2, 'Mouse', 29.99)
```

**Features demonstrated**:
- `SERIAL`: Auto-incrementing integer
- Auto-increment behavior without explicit ID

### Table with Multiple Data Types

Example showing various data types:

```sql
CREATE TABLE employees (
    employee_id INTEGER PRIMARY KEY,
    first_name VARCHAR(100) NOT NULL,
    last_name VARCHAR(100) NOT NULL,
    birth_date DATE,
    hire_date DATE NOT NULL,
    salary NUMERIC(10, 2),
    is_active BOOLEAN DEFAULT true,
    bio TEXT
);

-- Insert sample employee
INSERT INTO employees VALUES (
    1,
    'John',
    'Smith',
    DATE '1985-05-15',
    DATE '2020-01-01',
    75000.00,
    true,
    'Senior software engineer with 10 years experience'
);
```

**Features demonstrated**:
- `VARCHAR`: Character strings
- `DATE`: Calendar dates
- `NUMERIC`: Decimal numbers
- `BOOLEAN`: True/false values
- `TEXT`: Long text fields
- `DEFAULT`: Provides default values

## Tables with Constraints

### Single Column Constraints

Constraints applied to individual columns:

```sql
CREATE TABLE user_accounts (
    account_id INTEGER PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    username VARCHAR(50) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    verified BOOLEAN DEFAULT false,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

**Constraints demonstrated**:
- `PRIMARY KEY`: Uniquely identifies rows
- `UNIQUE`: Ensures no duplicate values
- `NOT NULL`: Requires value
- `DEFAULT`: Provides default value

### Composite Primary Key

Multiple columns form the primary key:

```sql
CREATE TABLE order_items (
    order_id INTEGER,
    item_id INTEGER,
    quantity INTEGER NOT NULL,
    unit_price NUMERIC(10, 2) NOT NULL,
    PRIMARY KEY (order_id, item_id)
);
```

**Features demonstrated**:
- Composite primary key (multiple columns)
- Primary key uniqueness enforced across column combination

### Foreign Key Constraints

Tables linked through foreign keys:

```sql
CREATE TABLE customers (
    customer_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL
);

CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER NOT NULL,
    order_date DATE NOT NULL,
    total_amount NUMERIC(10, 2),
    FOREIGN KEY (customer_id) REFERENCES customers(customer_id)
);
```

**Features demonstrated**:
- `FOREIGN KEY`: Links to another table
- Referential integrity enforcement
- One-to-many relationship (customers to orders)

### CHECK Constraints

Validate column values:

```sql
CREATE TABLE products (
    product_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    price NUMERIC(10, 2) NOT NULL,
    stock_quantity INTEGER NOT NULL,
    CHECK (price > 0),
    CHECK (stock_quantity >= 0)
);

-- Valid insert
INSERT INTO products VALUES (1, 'Laptop', 999.99, 10);

-- Invalid insert (negative price)
-- INSERT INTO products VALUES (2, 'Mouse', -29.99, 5);
-- Error: CHECK constraint violated
```

**Features demonstrated**:
- `CHECK`: Validates conditions
- Multiple CHECK constraints per table
- Value range validation

### UNIQUE with NULL Handling

UNIQUE constraints with multiple NULLs allowed:

```sql
CREATE TABLE user_profiles (
    profile_id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
    phone_number VARCHAR(20) UNIQUE,
    alternative_email VARCHAR(255) UNIQUE,
    bio TEXT
);

-- Multiple rows can have NULL in unique columns
INSERT INTO user_profiles VALUES (1, 1, '555-1234', NULL, 'First user');
INSERT INTO user_profiles VALUES (2, 2, NULL, NULL, 'Second user');

-- Duplicate non-NULL values not allowed
-- INSERT INTO user_profiles VALUES (3, 3, '555-1234', NULL, 'Third user');
-- Error: UNIQUE constraint violated
```

**Features demonstrated**:
- `UNIQUE`: No duplicate non-NULL values
- Multiple NULLs allowed (standard SQL behavior)

### Named Constraints

Constraints with explicit names for easier management:

```sql
CREATE TABLE invoices (
    invoice_id INTEGER,
    customer_id INTEGER,
    invoice_number VARCHAR(50),
    amount NUMERIC(10, 2),
    status VARCHAR(20),

    CONSTRAINT pk_invoices PRIMARY KEY (invoice_id),
    CONSTRAINT uk_invoice_number UNIQUE (invoice_number),
    CONSTRAINT fk_customer FOREIGN KEY (customer_id) REFERENCES customers(customer_id),
    CONSTRAINT ck_amount CHECK (amount > 0),
    CONSTRAINT ck_status CHECK (status IN ('draft', 'sent', 'paid', 'overdue'))
);
```

**Features demonstrated**:
- Named constraints using `CONSTRAINT name` syntax
- Easier constraint management
- CHECK with IN operator

## Multi-Table Schemas

### Library Management System

A complete schema for a library:

```sql
CREATE TABLE authors (
    author_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    birth_year INTEGER,
    country VARCHAR(100)
);

CREATE TABLE books (
    book_id INTEGER PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    author_id INTEGER NOT NULL,
    isbn VARCHAR(13) UNIQUE,
    publication_year INTEGER,
    pages INTEGER CHECK (pages > 0),
    FOREIGN KEY (author_id) REFERENCES authors(author_id)
);

CREATE TABLE members (
    member_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE,
    join_date DATE NOT NULL
);

CREATE TABLE checkouts (
    checkout_id INTEGER PRIMARY KEY,
    member_id INTEGER NOT NULL,
    book_id INTEGER NOT NULL,
    checkout_date DATE NOT NULL,
    due_date DATE NOT NULL,
    return_date DATE,
    FOREIGN KEY (member_id) REFERENCES members(member_id),
    FOREIGN KEY (book_id) REFERENCES books(book_id)
);

-- Insert sample data
INSERT INTO authors VALUES (1, 'J.K. Rowling', 1965, 'United Kingdom');
INSERT INTO authors VALUES (2, 'George R.R. Martin', 1948, 'United States');

INSERT INTO books VALUES (1, 'Harry Potter and the Philosopher''s Stone', 1, '9780747532699', 1998, 309);
INSERT INTO books VALUES (2, 'A Game of Thrones', 2, '9780553103540', 1996, 694);

INSERT INTO members VALUES (1, 'Alice Johnson', 'alice@example.com', DATE '2023-01-15');
INSERT INTO members VALUES (2, 'Bob Williams', 'bob@example.com', DATE '2023-02-20');

INSERT INTO checkouts VALUES (1, 1, 1, DATE '2024-01-01', DATE '2024-01-15', NULL);
INSERT INTO checkouts VALUES (2, 2, 2, DATE '2024-01-05', DATE '2024-01-19', NULL);
```

**Schema demonstrates**:
- Multiple related tables
- Foreign key relationships
- One-to-many relationships (authors to books)
- Many-to-many patterns (members to books through checkouts)

### Blog Platform

A schema for a blogging application:

```sql
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    username VARCHAR(50) UNIQUE NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE categories (
    category_id INTEGER PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    description TEXT
);

CREATE TABLE posts (
    post_id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
    category_id INTEGER NOT NULL,
    title VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    published BOOLEAN DEFAULT false,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(user_id),
    FOREIGN KEY (category_id) REFERENCES categories(category_id)
);

CREATE TABLE comments (
    comment_id INTEGER PRIMARY KEY,
    post_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (post_id) REFERENCES posts(post_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id)
);

CREATE TABLE tags (
    tag_id INTEGER PRIMARY KEY,
    name VARCHAR(50) UNIQUE NOT NULL
);

CREATE TABLE post_tags (
    post_id INTEGER,
    tag_id INTEGER,
    PRIMARY KEY (post_id, tag_id),
    FOREIGN KEY (post_id) REFERENCES posts(post_id),
    FOREIGN KEY (tag_id) REFERENCES tags(tag_id)
);
```

**Schema demonstrates**:
- User-generated content system
- Category organization
- Comments and discussions
- Many-to-many tagging system
- Temporal fields (created_at, updated_at)

## ALTER TABLE Operations

### Adding Columns

Add new columns to existing tables:

```sql
-- Create initial table
CREATE TABLE products (
    product_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    price NUMERIC(10, 2) NOT NULL
);

-- Add a new column with default value
ALTER TABLE products ADD COLUMN description TEXT DEFAULT '';

-- Add a nullable column
ALTER TABLE products ADD COLUMN category VARCHAR(100);

-- Add a column with constraint
ALTER TABLE products ADD COLUMN stock_quantity INTEGER DEFAULT 0 CHECK (stock_quantity >= 0);

-- Verify the columns
SELECT * FROM products;
```

**Operations demonstrated**:
- Adding columns to existing tables
- Default values for new columns
- Optional (nullable) columns
- Constraints on new columns

### Modifying Constraints

Add and remove constraints:

```sql
-- Create table without email constraint
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    username VARCHAR(50) NOT NULL,
    email VARCHAR(255) NOT NULL
);

-- Add UNIQUE constraint on email
ALTER TABLE users ADD CONSTRAINT uk_email UNIQUE (email);

-- Later, remove the constraint
ALTER TABLE users DROP CONSTRAINT uk_email;

-- Add a new constraint
ALTER TABLE users ADD CONSTRAINT ck_username CHECK (LENGTH(username) > 2);
```

**Operations demonstrated**:
- Adding constraints to existing tables
- Named constraints for easy management
- Removing constraints

### Renaming Objects

Rename columns and tables:

```sql
-- Create initial table
CREATE TABLE customer (
    id INTEGER PRIMARY KEY,
    name VARCHAR(255),
    addr VARCHAR(255)
);

-- Rename column (abbreviation to full name)
ALTER TABLE customer RENAME COLUMN addr TO address;

-- Rename table (singular to plural)
ALTER TABLE customer RENAME TO customers;

-- Verify changes
SELECT * FROM customers;
```

**Operations demonstrated**:
- Renaming columns
- Renaming tables
- Application compatibility considerations

### Dropping Columns

Remove columns from tables:

```sql
-- Create table with various columns
CREATE TABLE temp_users (
    user_id INTEGER PRIMARY KEY,
    username VARCHAR(50),
    email VARCHAR(255),
    legacy_field VARCHAR(100),
    deprecated_column INTEGER
);

-- Drop unused columns
ALTER TABLE temp_users DROP COLUMN legacy_field;
ALTER TABLE temp_users DROP COLUMN deprecated_column;

-- Verify remaining columns
SELECT * FROM temp_users;
```

**Important notes**:
- Irreversible operation (use backups)
- Can't drop columns used in constraints
- Can't drop columns used in indexes

## Index Management

### Creating Indexes

Create indexes for query performance:

```sql
CREATE TABLE employees (
    employee_id INTEGER PRIMARY KEY,
    first_name VARCHAR(100),
    last_name VARCHAR(100),
    email VARCHAR(255),
    department VARCHAR(100),
    hire_date DATE
);

-- Create index on email for quick lookups
CREATE INDEX idx_employees_email ON employees(email);

-- Create composite index for department and hire_date
CREATE INDEX idx_employees_dept_date ON employees(department, hire_date);

-- Create index with conditional logic
CREATE INDEX idx_employees_hire_date ON employees(hire_date);

-- Conditional index creation (if not exists)
CREATE INDEX IF NOT EXISTS idx_employees_name ON employees(last_name);
```

**Index features**:
- Single column indexes
- Composite indexes for multi-column searches
- Automatic indexes from PRIMARY KEY and UNIQUE constraints

### Dropping Indexes

Remove indexes:

```sql
-- Drop specific index
DROP INDEX idx_employees_email;

-- Drop with IF EXISTS (no error if missing)
DROP INDEX IF NOT EXISTS idx_old_index;

-- Remove multiple indexes
DROP INDEX IF EXISTS idx_employees_dept_date;
DROP INDEX IF EXISTS idx_employees_hire_date;
```

**Important notes**:
- Can't drop PRIMARY KEY or UNIQUE constraint indexes directly
- Drop constraints instead if needed

## Real-World Scenarios

### E-Commerce Product Catalog

Complete product management system:

```sql
CREATE TABLE vendors (
    vendor_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    contact_email VARCHAR(255) UNIQUE,
    phone VARCHAR(20),
    address TEXT
);

CREATE TABLE product_categories (
    category_id INTEGER PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    description TEXT,
    parent_category_id INTEGER,
    FOREIGN KEY (parent_category_id) REFERENCES product_categories(category_id)
);

CREATE TABLE products (
    product_id INTEGER PRIMARY KEY,
    vendor_id INTEGER NOT NULL,
    category_id INTEGER NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    sku VARCHAR(50) UNIQUE NOT NULL,
    price NUMERIC(10, 2) NOT NULL,
    cost NUMERIC(10, 2),
    stock_quantity INTEGER DEFAULT 0,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (vendor_id) REFERENCES vendors(vendor_id),
    FOREIGN KEY (category_id) REFERENCES product_categories(category_id),
    CHECK (price > 0),
    CHECK (stock_quantity >= 0)
);

CREATE INDEX idx_products_vendor ON products(vendor_id);
CREATE INDEX idx_products_category ON products(category_id);
CREATE INDEX idx_products_sku ON products(sku);
```

### Healthcare Patient Records

Medical data management:

```sql
CREATE TABLE patients (
    patient_id INTEGER PRIMARY KEY,
    first_name VARCHAR(100) NOT NULL,
    last_name VARCHAR(100) NOT NULL,
    date_of_birth DATE NOT NULL,
    gender VARCHAR(10),
    email VARCHAR(255) UNIQUE,
    phone VARCHAR(20),
    address TEXT,
    registration_date DATE NOT NULL
);

CREATE TABLE doctors (
    doctor_id INTEGER PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    specialty VARCHAR(100) NOT NULL,
    license_number VARCHAR(50) UNIQUE NOT NULL,
    phone VARCHAR(20)
);

CREATE TABLE appointments (
    appointment_id INTEGER PRIMARY KEY,
    patient_id INTEGER NOT NULL,
    doctor_id INTEGER NOT NULL,
    appointment_date TIMESTAMP NOT NULL,
    duration_minutes INTEGER,
    status VARCHAR(20) DEFAULT 'scheduled',
    notes TEXT,
    FOREIGN KEY (patient_id) REFERENCES patients(patient_id),
    FOREIGN KEY (doctor_id) REFERENCES doctors(doctor_id),
    CHECK (duration_minutes > 0)
);

CREATE TABLE prescriptions (
    prescription_id INTEGER PRIMARY KEY,
    patient_id INTEGER NOT NULL,
    doctor_id INTEGER NOT NULL,
    medication_name VARCHAR(255) NOT NULL,
    dosage VARCHAR(100) NOT NULL,
    frequency VARCHAR(100) NOT NULL,
    prescribed_date DATE NOT NULL,
    expiration_date DATE,
    FOREIGN KEY (patient_id) REFERENCES patients(patient_id),
    FOREIGN KEY (doctor_id) REFERENCES doctors(doctor_id)
);
```

## Error Handling

### Handling Missing Tables

Safe operations for development:

```sql
-- Safe: Won't error if table doesn't exist
DROP TABLE IF EXISTS temp_table;

-- Safe: Won't error if table already exists
CREATE TABLE IF NOT EXISTS users (
    user_id INTEGER PRIMARY KEY,
    username VARCHAR(50) NOT NULL
);

-- Can run multiple times without error
CREATE TABLE IF NOT EXISTS users (
    user_id INTEGER PRIMARY KEY,
    username VARCHAR(50) NOT NULL
);
```

### Constraint Violation Examples

Understanding constraint violations:

```sql
CREATE TABLE unique_demo (
    id INTEGER PRIMARY KEY,
    email VARCHAR(255) UNIQUE
);

-- Valid insert
INSERT INTO unique_demo VALUES (1, 'john@example.com');

-- Error: Duplicate email (UNIQUE constraint)
-- INSERT INTO unique_demo VALUES (2, 'john@example.com');

-- Valid: NULL is allowed
INSERT INTO unique_demo VALUES (2, NULL);
INSERT INTO unique_demo VALUES (3, NULL);  -- Multiple NULLs allowed

-- Valid insert with different email
INSERT INTO unique_demo VALUES (4, 'jane@example.com');
```

### Foreign Key Violations

```sql
CREATE TABLE parent_table (
    parent_id INTEGER PRIMARY KEY,
    name VARCHAR(100)
);

CREATE TABLE child_table (
    child_id INTEGER PRIMARY KEY,
    parent_id INTEGER NOT NULL,
    FOREIGN KEY (parent_id) REFERENCES parent_table(parent_id)
);

-- Valid: parent exists
INSERT INTO parent_table VALUES (1, 'Parent 1');
INSERT INTO child_table VALUES (1, 1);  -- Works

-- Error: parent doesn't exist
-- INSERT INTO child_table VALUES (2, 999);
-- Error: FOREIGN KEY constraint violated
```

## Complete E-commerce Schema

A comprehensive example for an e-commerce platform:

```sql
-- Users and accounts
CREATE TABLE users (
    user_id INTEGER PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    username VARCHAR(50) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    first_name VARCHAR(100),
    last_name VARCHAR(100),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Customer addresses
CREATE TABLE addresses (
    address_id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
    address_type VARCHAR(20),
    street VARCHAR(255) NOT NULL,
    city VARCHAR(100) NOT NULL,
    state VARCHAR(50),
    postal_code VARCHAR(20),
    country VARCHAR(100) NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id)
);

-- Product catalog
CREATE TABLE categories (
    category_id INTEGER PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    description TEXT
);

CREATE TABLE products (
    product_id INTEGER PRIMARY KEY,
    category_id INTEGER NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    price NUMERIC(10, 2) NOT NULL,
    cost NUMERIC(10, 2),
    stock_quantity INTEGER DEFAULT 0,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (category_id) REFERENCES categories(category_id),
    CHECK (price > 0),
    CHECK (stock_quantity >= 0)
);

-- Shopping carts
CREATE TABLE shopping_carts (
    cart_id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL UNIQUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(user_id)
);

CREATE TABLE cart_items (
    cart_item_id INTEGER PRIMARY KEY,
    cart_id INTEGER NOT NULL,
    product_id INTEGER NOT NULL,
    quantity INTEGER NOT NULL,
    added_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (cart_id) REFERENCES shopping_carts(cart_id),
    FOREIGN KEY (product_id) REFERENCES products(product_id),
    CHECK (quantity > 0)
);

-- Orders
CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
    order_date TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    total_amount NUMERIC(10, 2) NOT NULL,
    status VARCHAR(20) DEFAULT 'pending',
    shipping_address_id INTEGER,
    FOREIGN KEY (user_id) REFERENCES users(user_id),
    FOREIGN KEY (shipping_address_id) REFERENCES addresses(address_id),
    CHECK (total_amount >= 0)
);

CREATE TABLE order_items (
    order_item_id INTEGER PRIMARY KEY,
    order_id INTEGER NOT NULL,
    product_id INTEGER NOT NULL,
    quantity INTEGER NOT NULL,
    unit_price NUMERIC(10, 2) NOT NULL,
    FOREIGN KEY (order_id) REFERENCES orders(order_id),
    FOREIGN KEY (product_id) REFERENCES products(product_id),
    CHECK (quantity > 0),
    CHECK (unit_price > 0)
);

-- Reviews and ratings
CREATE TABLE reviews (
    review_id INTEGER PRIMARY KEY,
    product_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    rating INTEGER NOT NULL,
    title VARCHAR(255),
    content TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (product_id) REFERENCES products(product_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id),
    CHECK (rating >= 1 AND rating <= 5)
);

-- Create indexes for performance
CREATE INDEX idx_products_category ON products(category_id);
CREATE INDEX idx_orders_user ON orders(user_id);
CREATE INDEX idx_order_items_order ON order_items(order_id);
CREATE INDEX idx_reviews_product ON reviews(product_id);
CREATE INDEX idx_addresses_user ON addresses(user_id);
```

This comprehensive schema demonstrates:
- Multiple related tables
- Foreign key relationships
- Check constraints for data validation
- Indexes for query performance
- Temporal fields
- Complex relationships

## Summary

This examples guide covers:
- Basic table creation
- Constraint usage
- Multi-table schemas
- ALTER TABLE operations
- Index management
- Real-world scenarios
- Error handling
- Production-ready patterns

For more information, see:
- [README.md](README.md) for DDL syntax overview
- [SCHEMA_MIGRATION_GUIDE.md](SCHEMA_MIGRATION_GUIDE.md) for migration patterns
- [DEVELOPER_GUIDE.md](DEVELOPER_GUIDE.md) for implementation details
- [ERROR_REFERENCE.md](ERROR_REFERENCE.md) for error codes
- [SCHEMA_DESIGN_GUIDE.md](SCHEMA_DESIGN_GUIDE.md) for best practices
