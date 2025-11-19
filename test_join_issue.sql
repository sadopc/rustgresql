-- Create test tables for JOIN issue
CREATE TABLE departments (
    id INTEGER PRIMARY KEY,
    name VARCHAR(100) NOT NULL
);

CREATE TABLE employees (
    id INTEGER PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    department_id INTEGER,
    salary INTEGER,
    hire_date DATE,
    email VARCHAR(100),
    is_active BOOLEAN,
    manager_id INTEGER
);

-- Insert test data
INSERT INTO departments VALUES (1, 'Engineering'), (2, 'Sales'), (3, 'Marketing'), (4, 'HR'), (5, 'Finance');

INSERT INTO employees VALUES
(1, 'Alice Johnson', 1, 95000, '2020-01-15', 'alice@example.com', true, null),
(2, 'Bob Smith', 1, 88000, '2020-03-20', 'bob@example.com', true, 1),
(3, 'Carol White', 2, 75000, '2019-06-10', 'carol@example.com', true, null),
(4, 'David Brown', 2, 70000, '2021-02-01', 'david@example.com', true, 3),
(5, 'Eve Davis', 3, 65000, '2021-05-15', 'eve@example.com', true, null),
(6, 'Frank Miller', 3, 60000, '2022-01-10', 'frank@example.com', true, 5),
(7, 'Grace Lee', 1, 90000, '2019-09-01', 'grace@example.com', true, 1),
(8, 'Henry Wilson', 4, 55000, '2022-03-15', 'henry@example.com', true, null),
(9, 'Iris Taylor', 5, 80000, '2020-07-20', 'iris@example.com', true, null),
(10, 'Jack Anderson', 1, 78000, '2021-11-05', 'jack@example.com', true, 1),
(11, 'Kate Martinez', 2, 72000, '2023-01-15', 'kate@example.com', true, null);

-- Test the queries
SELECT * FROM employees;
SELECT e.name, d.name AS department_name FROM employees e INNER JOIN departments d ON e.department_id = d.id;
SELECT e.name, d.name AS department_name FROM employees e LEFT JOIN departments d ON e.department_id = d.id;