#!/bin/bash

# Test script for ORDER BY and LIMIT functionality
cat << 'EOF' | ./target/debug/rustgresql
CREATE TABLE employees (
    id INTEGER,
    name TEXT,
    salary INTEGER
);

INSERT INTO employees (id, name, salary) VALUES
    (1, 'Alice Johnson', 95000),
    (2, 'Bob Smith', 88000),
    (3, 'Carol White', 75000),
    (4, 'David Brown', 70000),
    (5, 'Eve Davis', 65000),
    (6, 'Frank Miller', 60000),
    (7, 'Grace Lee', 90000),
    (8, 'Henry Wilson', 55000),
    (9, 'Iris Taylor', 80000),
    (10, 'Jack Anderson', 78000),
    (11, 'Kate Martinez', 72000);

-- Test basic SELECT
SELECT name, salary FROM employees;

-- Test ORDER BY only
SELECT name, salary FROM employees ORDER BY salary DESC;

-- Test LIMIT only
SELECT name, salary FROM employees LIMIT 5;

-- Test ORDER BY + LIMIT (the problematic query)
SELECT name, salary FROM employees ORDER BY salary DESC LIMIT 5;
EOF