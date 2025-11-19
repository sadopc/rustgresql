#!/bin/bash

# Simple test for ORDER BY and LIMIT functionality
cat << 'EOF' | ./target/debug/rustgresql
CREATE TABLE employees (
    id INTEGER,
    name TEXT,
    salary INTEGER
);
EOF

cat << 'EOF' | ./target/debug/rustgresql
INSERT INTO employees (id, name, salary) VALUES (1, 'Alice Johnson', 95000);
EOF

cat << 'EOF' | ./target/debug/rustgresql
INSERT INTO employees (id, name, salary) VALUES (2, 'Bob Smith', 88000);
EOF

cat << 'EOF' | ./target/debug/rustgresql
INSERT INTO employees (id, name, salary) VALUES (3, 'Carol White', 75000);
EOF

cat << 'EOF' | ./target/debug/rustgresql
INSERT INTO employees (id, name, salary) VALUES (4, 'Grace Lee', 90000);
EOF

cat << 'EOF' | ./target/debug/rustgresql
INSERT INTO employees (id, name, salary) VALUES (5, 'Henry Wilson', 55000);
EOF

echo "Testing basic SELECT:"
cat << 'EOF' | ./target/debug/rustgresql
SELECT name, salary FROM employees;
EOF

echo "Testing ORDER BY:"
cat << 'EOF' | ./target/debug/rustgresql
SELECT name, salary FROM employees ORDER BY salary DESC;
EOF

echo "Testing LIMIT:"
cat << 'EOF' | ./target/debug/rustgresql
SELECT name, salary FROM employees LIMIT 3;
EOF

echo "Testing ORDER BY + LIMIT:"
cat << 'EOF' | ./target/debug/rustgresql
SELECT name, salary FROM employees ORDER BY salary DESC LIMIT 3;
EOF