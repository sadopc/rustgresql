-- ==================================================
-- RustgreSQL Index Test Suite
-- ==================================================

-- Test 1: Create a table for index testing
-- ==================================================
CREATE TABLE customers (
    customer_id INTEGER,
    first_name VARCHAR(50),
    last_name VARCHAR(50),
    email VARCHAR(100),
    city VARCHAR(50)
);

-- Test 2: Insert customer data
-- ==================================================
INSERT INTO customers VALUES (1, 'John', 'Smith', 'john.smith@email.com', 'New York');
INSERT INTO customers VALUES (2, 'Mary', 'Johnson', 'mary.j@email.com', 'Los Angeles');
INSERT INTO customers VALUES (3, 'Robert', 'Williams', 'robert.w@email.com', 'Chicago');
INSERT INTO customers VALUES (4, 'Patricia', 'Brown', 'patricia.b@email.com', 'Houston');
INSERT INTO customers VALUES (5, 'Michael', 'Davis', 'michael.d@email.com', 'Phoenix');
INSERT INTO customers VALUES (6, 'Linda', 'Miller', 'linda.m@email.com', 'Philadelphia');
INSERT INTO customers VALUES (7, 'James', 'Wilson', 'james.w@email.com', 'San Antonio');
INSERT INTO customers VALUES (8, 'Barbara', 'Moore', 'barbara.m@email.com', 'San Diego');

-- Test 3: Create an index on customer_id
-- ==================================================
CREATE INDEX idx_customer_id ON customers (customer_id);

-- Test 4: Query using indexed column
-- ==================================================
SELECT * FROM customers WHERE customer_id = 5;

-- Test 5: Create index on email
-- ==================================================
CREATE INDEX idx_email ON customers (email);

-- Test 6: Query using email index
-- ==================================================
SELECT first_name, last_name FROM customers WHERE email = 'mary.j@email.com';

-- Test 7: Create index on city
-- ==================================================
CREATE INDEX idx_city ON customers (city);

-- Test 8: Query by city
-- ==================================================
SELECT first_name, last_name, city FROM customers WHERE city = 'Chicago';

-- Test 9: List all customers
-- ==================================================
SELECT * FROM customers;

-- Test 10: Drop an index
-- ==================================================
DROP INDEX idx_email;

-- Test 11: Query after dropping index (should still work)
-- ==================================================
SELECT * FROM customers WHERE email = 'john.smith@email.com';
