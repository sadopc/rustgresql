-- ==================================================
-- RustgreSQL Basic Test Suite
-- ==================================================
-- Run these commands one by one or in batches
-- ==================================================

-- Test 1: Create a simple table
-- ==================================================
CREATE TABLE employees (
    id INTEGER,
    name VARCHAR(100),
    department VARCHAR(50),
    salary INTEGER,
    is_manager BOOLEAN
);

-- Test 2: Insert data
-- ==================================================
INSERT INTO employees (id, name, department, salary, is_manager)
VALUES (1, 'John Doe', 'Engineering', 75000, false);

INSERT INTO employees (id, name, department, salary, is_manager)
VALUES (2, 'Jane Smith', 'Engineering', 85000, true);

INSERT INTO employees (id, name, department, salary, is_manager)
VALUES (3, 'Bob Wilson', 'Sales', 60000, false);

INSERT INTO employees (id, name, department, salary, is_manager)
VALUES (4, 'Alice Brown', 'Sales', 70000, true);

INSERT INTO employees (id, name, department, salary, is_manager)
VALUES (5, 'Charlie Davis', 'HR', 55000, false);

-- Test 3: Simple SELECT
-- ==================================================
SELECT * FROM employees;

-- Test 4: SELECT specific columns
-- ==================================================
SELECT name, department FROM employees;

-- Test 5: WHERE clause with integer comparison
-- ==================================================
SELECT name, salary FROM employees WHERE salary > 60000;

-- Test 6: WHERE clause with boolean
-- ==================================================
SELECT name, department FROM employees WHERE is_manager = true;

-- Test 7: WHERE clause with string equality
-- ==================================================
SELECT name, salary FROM employees WHERE department = 'Engineering';

-- Test 8: COUNT aggregate
-- ==================================================
SELECT COUNT(*) FROM employees;

-- Test 9: COUNT with WHERE
-- ==================================================
SELECT COUNT(*) FROM employees WHERE department = 'Sales';

-- Test 10: UPDATE operation
-- ==================================================
UPDATE employees SET salary = 80000 WHERE id = 1;

-- Test 11: Verify UPDATE
-- ==================================================
SELECT name, salary FROM employees WHERE id = 1;

-- Test 12: DELETE operation
-- ==================================================
DELETE FROM employees WHERE id = 5;

-- Test 13: Verify DELETE
-- ==================================================
SELECT * FROM employees;
