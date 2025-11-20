-- Create test tables for queries
CREATE TABLE employees (id INTEGER PRIMARY KEY, name VARCHAR(100) NOT NULL, department_id INTEGER, salary NUMERIC(10, 2), hire_date DATE, email VARCHAR(100), is_active BOOLEAN DEFAULT TRUE, manager_id INTEGER);
CREATE TABLE departments (id INTEGER PRIMARY KEY, name VARCHAR(100) NOT NULL, budget NUMERIC(12, 2), location VARCHAR(100));
CREATE TABLE projects (id INTEGER PRIMARY KEY, name VARCHAR(200) NOT NULL, department_id INTEGER, start_date DATE, end_date DATE, budget NUMERIC(12, 2));
CREATE TABLE employee_projects (employee_id INTEGER, project_id INTEGER, role VARCHAR(50), hours_allocated INTEGER, PRIMARY KEY (employee_id, project_id));
CREATE TABLE salaries_history (id INTEGER PRIMARY KEY, employee_id INTEGER, salary NUMERIC(10, 2), effective_date DATE);

-- Insert sample data
INSERT INTO departments (id, name, budget, location) VALUES (1, 'Engineering', 1000000.00, 'New York'), (2, 'Sales', 500000.00, 'San Francisco'), (3, 'Marketing', 300000.00, 'Los Angeles'), (4, 'HR', 200000.00, 'New York'), (5, 'Finance', 400000.00, 'Chicago');
INSERT INTO employees (id, name, department_id, salary, hire_date, email, is_active, manager_id) VALUES (1, 'Alice Johnson', 1, 95000.00, '2020-01-15', 'alice@example.com', TRUE, NULL), (2, 'Bob Smith', 1, 85000.00, '2020-03-20', 'bob@example.com', TRUE, 1), (3, 'Carol White', 2, 75000.00, '2019-06-10', 'carol@example.com', TRUE, NULL), (4, 'David Brown', 2, 70000.00, '2021-02-01', 'david@example.com', TRUE, 3), (5, 'Eve Davis', 3, 65000.00, '2021-05-15', 'eve@example.com', TRUE, NULL), (6, 'Frank Miller', 3, 60000.00, '2022-01-10', 'frank@example.com', TRUE, 5), (7, 'Grace Lee', 1, 90000.00, '2019-09-01', 'grace@example.com', TRUE, 1), (8, 'Henry Wilson', 4, 55000.00, '2022-03-15', 'henry@example.com', TRUE, NULL), (9, 'Iris Taylor', 5, 80000.00, '2020-07-20', 'iris@example.com', TRUE, NULL), (10, 'Jack Anderson', 1, 78000.00, '2021-11-05', 'jack@example.com', FALSE, 1);

-- Test the fixed query
SELECT name, salary, (SELECT AVG(salary) FROM employees) AS avg_salary FROM employees;