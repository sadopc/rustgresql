-- ============================================================================
-- RustgreSQL Comprehensive Test Query Suite
-- 100 Queries Testing All Major Database Features
-- ============================================================================

-- First, let's create a comprehensive test schema
-- ============================================================================
-- SECTION 0: Test Schema Setup
-- ============================================================================

-- Create test tables for queries
CREATE TABLE employees (id INTEGER PRIMARY KEY, name VARCHAR(100) NOT NULL, department_id INTEGER, salary NUMERIC(10, 2), hire_date DATE, email VARCHAR(100), is_active BOOLEAN DEFAULT TRUE, manager_id INTEGER);

CREATE TABLE departments (id INTEGER PRIMARY KEY, name VARCHAR(100) NOT NULL, budget NUMERIC(12, 2), location VARCHAR(100));

CREATE TABLE projects (id INTEGER PRIMARY KEY, name VARCHAR(200) NOT NULL, department_id INTEGER, start_date DATE, end_date DATE, budget NUMERIC(12, 2));

CREATE TABLE employee_projects (employee_id INTEGER, project_id INTEGER, role VARCHAR(50), hours_allocated INTEGER, PRIMARY KEY (employee_id, project_id));

CREATE TABLE salaries_history (id INTEGER PRIMARY KEY, employee_id INTEGER, salary NUMERIC(10, 2), effective_date DATE);

-- Insert sample data
INSERT INTO departments (id, name, budget, location) VALUES (1, 'Engineering', 1000000.00, 'New York'), (2, 'Sales', 500000.00, 'San Francisco'), (3, 'Marketing', 300000.00, 'Los Angeles'), (4, 'HR', 200000.00, 'New York'), (5, 'Finance', 400000.00, 'Chicago');

INSERT INTO employees (id, name, department_id, salary, hire_date, email, is_active, manager_id) VALUES (1, 'Alice Johnson', 1, 95000.00, '2020-01-15', 'alice@example.com', TRUE, NULL), (2, 'Bob Smith', 1, 85000.00, '2020-03-20', 'bob@example.com', TRUE, 1), (3, 'Carol White', 2, 75000.00, '2019-06-10', 'carol@example.com', TRUE, NULL), (4, 'David Brown', 2, 70000.00, '2021-02-01', 'david@example.com', TRUE, 3), (5, 'Eve Davis', 3, 65000.00, '2021-05-15', 'eve@example.com', TRUE, NULL), (6, 'Frank Miller', 3, 60000.00, '2022-01-10', 'frank@example.com', TRUE, 5), (7, 'Grace Lee', 1, 90000.00, '2019-09-01', 'grace@example.com', TRUE, 1), (8, 'Henry Wilson', 4, 55000.00, '2022-03-15', 'henry@example.com', TRUE, NULL), (9, 'Iris Taylor', 5, 80000.00, '2020-07-20', 'iris@example.com', TRUE, NULL), (10, 'Jack Anderson', 1, 78000.00, '2021-11-05', 'jack@example.com', FALSE, 1);

INSERT INTO projects (id, name, department_id, start_date, end_date, budget) VALUES (1, 'Project Alpha', 1, '2023-01-01', '2023-06-30', 250000.00), (2, 'Project Beta', 1, '2023-03-01', '2023-12-31', 500000.00), (3, 'Sales Campaign Q1', 2, '2023-01-01', '2023-03-31', 100000.00), (4, 'Marketing Initiative', 3, '2023-02-01', '2023-08-31', 150000.00), (5, 'HR System Upgrade', 4, '2023-01-15', '2023-05-15', 75000.00);

INSERT INTO employee_projects (employee_id, project_id, role, hours_allocated) VALUES (1, 1, 'Lead Developer', 160), (2, 1, 'Developer', 160), (7, 1, 'Developer', 120), (1, 2, 'Architect', 80), (2, 2, 'Lead Developer', 160), (10, 2, 'Developer', 160), (3, 3, 'Sales Lead', 160), (4, 3, 'Sales Rep', 160), (5, 4, 'Marketing Manager', 160), (6, 4, 'Marketing Coordinator', 160), (8, 5, 'HR Specialist', 160);

INSERT INTO salaries_history (id, employee_id, salary, effective_date) VALUES (1, 1, 85000.00, '2020-01-15'), (2, 1, 90000.00, '2021-01-15'), (3, 1, 95000.00, '2022-01-15'), (4, 2, 75000.00, '2020-03-20'), (5, 2, 80000.00, '2021-03-20'), (6, 2, 85000.00, '2022-03-20'), (7, 3, 70000.00, '2019-06-10'), (8, 3, 75000.00, '2021-06-10');


-- ============================================================================
-- SECTION 1: Basic Operations (10 queries)
-- ============================================================================

-- Query 1: Simple SELECT all columns
SELECT * FROM employees;

-- Query 2: SELECT specific columns
SELECT name, salary, hire_date FROM employees;

-- Query 3: SELECT with DISTINCT
SELECT DISTINCT department_id FROM employees;

-- Query 4: Simple INSERT
INSERT INTO employees (id, name, department_id, salary, hire_date, email, is_active) VALUES (11, 'Kate Martinez', 2, 72000.00, '2023-01-15', 'kate@example.com', TRUE);

-- Query 5: UPDATE with WHERE clause
UPDATE employees SET salary = 88000.00 WHERE id = 2;

-- Query 6: DELETE with WHERE clause
DELETE FROM employees WHERE id = 11;

-- Query 7: SELECT with LIMIT
SELECT name, salary FROM employees ORDER BY salary DESC LIMIT 5;

-- Query 8: SELECT with OFFSET
SELECT name, salary FROM employees ORDER BY salary DESC LIMIT 5 OFFSET 3;

-- Query 9: SELECT with LIMIT and OFFSET
SELECT * FROM projects ORDER BY start_date LIMIT 3 OFFSET 1;

-- Query 10: COUNT all rows
SELECT COUNT(*) FROM employees;


-- ============================================================================
-- SECTION 2: WHERE Clause & Filtering (8 queries)
-- ============================================================================

-- Query 11: Equality comparison
SELECT * FROM employees WHERE department_id = 1;

-- Query 12: Less than comparison
SELECT name, salary FROM employees WHERE salary < 70000.00;

-- Query 13: Greater than or equal comparison
SELECT name, salary FROM employees WHERE salary >= 80000.00;

-- Query 14: LIKE pattern matching
SELECT name, email FROM employees WHERE email LIKE '%example.com';

-- Query 15: LIKE with wildcard in middle
SELECT name FROM employees WHERE name LIKE '%Smith%';

-- Query 16: IN operator
SELECT name, department_id FROM employees WHERE department_id IN (1, 2, 3);

-- Query 17: IS NULL check
SELECT * FROM employees WHERE manager_id IS NULL;

-- Query 18: IS NOT NULL check
SELECT name, manager_id FROM employees WHERE manager_id IS NOT NULL;


-- ============================================================================
-- SECTION 3: JOIN Operations (10 queries)
-- ============================================================================

-- Query 19: Simple INNER JOIN
SELECT e.name, d.name AS department_name FROM employees e INNER JOIN departments d ON e.department_id = d.id;

-- Query 20: LEFT JOIN
SELECT e.name, d.name AS department_name FROM employees e LEFT JOIN departments d ON e.department_id = d.id;

-- Query 21: RIGHT JOIN
SELECT e.name, d.name AS department_name FROM departments d RIGHT JOIN employees e ON e.department_id = d.id;

-- Query 22: FULL OUTER JOIN
SELECT e.name AS employee_name, d.name AS department_name FROM employees e FULL JOIN departments d ON e.department_id = d.id;

-- Query 23: Self-join (employees and their managers)
SELECT e.name AS employee, m.name AS manager FROM employees e LEFT JOIN employees m ON e.manager_id = m.id;

-- Query 24: Three-way join
SELECT e.name, d.name AS department, p.name AS project FROM employees e INNER JOIN departments d ON e.department_id = d.id INNER JOIN employee_projects ep ON e.id = ep.employee_id INNER JOIN projects p ON ep.project_id = p.id;

-- Query 25: Join with WHERE clause
SELECT e.name, d.name AS department_name, e.salary FROM employees e INNER JOIN departments d ON e.department_id = d.id WHERE e.salary > 75000.00;

-- Query 26: Multiple joins with aggregation
SELECT d.name AS department, COUNT(e.id) AS employee_count FROM departments d LEFT JOIN employees e ON d.id = e.department_id GROUP BY d.name;

-- Query 27: Join with complex conditions
SELECT e.name, p.name AS project_name, ep.role FROM employees e INNER JOIN employee_projects ep ON e.id = ep.employee_id INNER JOIN projects p ON ep.project_id = p.id WHERE ep.hours_allocated > 100;

-- Query 28: Cross join (Cartesian product)
SELECT d1.name AS dept1, d2.name AS dept2 FROM departments d1 CROSS JOIN departments d2 WHERE d1.id < d2.id LIMIT 10;


-- ============================================================================
-- SECTION 4: Aggregations & GROUP BY (10 queries)
-- ============================================================================

-- Query 29: COUNT aggregate
SELECT COUNT(*) AS total_employees FROM employees;

-- Query 30: SUM aggregate
SELECT SUM(salary) AS total_salary FROM employees;

-- Query 31: AVG aggregate
SELECT AVG(salary) AS average_salary FROM employees;

-- Query 32: MIN and MAX aggregates
SELECT MIN(salary) AS min_salary, MAX(salary) AS max_salary FROM employees;

-- Query 33: GROUP BY with COUNT
SELECT department_id, COUNT(*) AS employee_count FROM employees GROUP BY department_id;

-- Query 34: GROUP BY with SUM
SELECT department_id, SUM(salary) AS total_department_salary FROM employees GROUP BY department_id;

-- Query 35: GROUP BY with multiple aggregates
SELECT department_id, COUNT(*) AS emp_count, AVG(salary) AS avg_salary, MAX(salary) AS max_salary FROM employees GROUP BY department_id;

-- Query 36: HAVING clause
SELECT department_id, AVG(salary) AS avg_salary FROM employees GROUP BY department_id HAVING AVG(salary) > 70000.00;

-- Query 37: COUNT DISTINCT
SELECT COUNT(DISTINCT department_id) AS unique_departments FROM employees;

-- Query 38: GROUP BY with JOIN
SELECT d.name, COUNT(e.id) AS employee_count, AVG(e.salary) AS avg_salary FROM departments d LEFT JOIN employees e ON d.id = e.department_id GROUP BY d.name ORDER BY employee_count DESC;


-- ============================================================================
-- SECTION 5: Subqueries (8 queries)
-- ============================================================================

-- Query 39: Scalar subquery in SELECT
SELECT name, salary, (SELECT AVG(salary) FROM employees) AS avg_salary FROM employees;

-- Query 40: Subquery in WHERE with IN
SELECT name, salary FROM employees WHERE department_id IN (SELECT id FROM departments WHERE budget > 400000.00);

-- Query 41: Subquery with comparison operator
SELECT name, salary FROM employees WHERE salary > (SELECT AVG(salary) FROM employees);

-- Query 42: EXISTS subquery
SELECT d.name FROM departments d WHERE EXISTS (SELECT 1 FROM employees e WHERE e.department_id = d.id);

-- Query 43: NOT EXISTS subquery
SELECT d.name FROM departments d WHERE NOT EXISTS (SELECT 1 FROM employees e WHERE e.department_id = d.id);

-- Query 44: Correlated subquery
SELECT e.name, e.salary, (SELECT AVG(e2.salary) FROM employees e2 WHERE e2.department_id = e.department_id) AS dept_avg FROM employees e;

-- Query 45: Subquery in FROM clause (derived table)
SELECT dept_summary.department_id, dept_summary.avg_salary FROM (SELECT department_id, AVG(salary) AS avg_salary FROM employees GROUP BY department_id) AS dept_summary WHERE dept_summary.avg_salary > 75000.00;

-- Query 46: Nested subqueries
SELECT name, salary FROM employees WHERE salary > (SELECT AVG(salary) FROM employees WHERE department_id = (SELECT id FROM departments WHERE name = 'Engineering'));


-- ============================================================================
-- SECTION 6: CTEs (Common Table Expressions) (6 queries)
-- ============================================================================

-- Query 47: Simple CTE
WITH high_earners AS (SELECT * FROM employees WHERE salary > 80000.00) SELECT name, salary FROM high_earners ORDER BY salary DESC;

-- Query 48: CTE with aggregation
WITH dept_stats AS (SELECT department_id, COUNT(*) AS emp_count, AVG(salary) AS avg_salary FROM employees GROUP BY department_id) SELECT d.name, ds.emp_count, ds.avg_salary FROM dept_stats ds JOIN departments d ON ds.department_id = d.id;

-- Query 49: Multiple CTEs
WITH eng_dept AS (SELECT id FROM departments WHERE name = 'Engineering'), eng_employees AS (SELECT * FROM employees WHERE department_id IN (SELECT id FROM eng_dept)) SELECT name, salary FROM eng_employees ORDER BY salary DESC;

-- Query 50: CTE with JOIN
WITH employee_dept AS (SELECT e.name, e.salary, d.name AS dept_name FROM employees e JOIN departments d ON e.department_id = d.id) SELECT * FROM employee_dept WHERE salary > 70000.00;

-- Query 51: Recursive CTE (employee hierarchy)
WITH RECURSIVE employee_hierarchy AS (SELECT id, name, manager_id, 0 AS level FROM employees WHERE manager_id IS NULL UNION ALL SELECT e.id, e.name, e.manager_id, eh.level + 1 FROM employees e JOIN employee_hierarchy eh ON e.manager_id = eh.id) SELECT id, name, level FROM employee_hierarchy ORDER BY level, name;

-- Query 52: CTE used multiple times
WITH dept_budgets AS (SELECT id, name, budget FROM departments) SELECT d1.name, d1.budget, d2.budget AS comparison_budget FROM dept_budgets d1 CROSS JOIN dept_budgets d2 WHERE d1.id < d2.id;


-- ============================================================================
-- SECTION 7: Window Functions (8 queries)
-- ============================================================================

-- Query 53: ROW_NUMBER window function
SELECT name, salary, department_id, ROW_NUMBER() OVER (ORDER BY salary DESC) AS salary_rank FROM employees;

-- Query 54: RANK window function
SELECT name, salary, RANK() OVER (ORDER BY salary DESC) AS rank FROM employees;

-- Query 55: Window function with PARTITION BY
SELECT name, salary, department_id, ROW_NUMBER() OVER (PARTITION BY department_id ORDER BY salary DESC) AS dept_rank FROM employees;

-- Query 56: Multiple window functions
SELECT name, salary, department_id, ROW_NUMBER() OVER (PARTITION BY department_id ORDER BY salary DESC) AS dept_rank, AVG(salary) OVER (PARTITION BY department_id) AS dept_avg_salary FROM employees;

-- Query 57: Window function with ROWS frame
SELECT name, salary, hire_date, AVG(salary) OVER (ORDER BY hire_date ROWS BETWEEN 2 PRECEDING AND CURRENT ROW) AS moving_avg FROM employees;

-- Query 58: DENSE_RANK window function
SELECT name, salary, DENSE_RANK() OVER (ORDER BY salary DESC) AS dense_rank FROM employees;

-- Query 59: LAG window function
SELECT name, salary, hire_date, LAG(salary, 1) OVER (ORDER BY hire_date) AS previous_hire_salary FROM employees;

-- Query 60: LEAD window function
SELECT name, salary, hire_date, LEAD(salary, 1) OVER (ORDER BY hire_date) AS next_hire_salary FROM employees;


-- ============================================================================
-- SECTION 8: Set Operations (4 queries)
-- ============================================================================

-- Query 61: UNION (removes duplicates)
SELECT name FROM employees WHERE department_id = 1 UNION SELECT name FROM employees WHERE salary > 80000.00;

-- Query 62: UNION ALL (keeps duplicates)
SELECT department_id FROM employees WHERE salary > 70000.00 UNION ALL SELECT department_id FROM employees WHERE is_active = TRUE;

-- Query 63: INTERSECT
SELECT id FROM employees WHERE salary > 75000.00 INTERSECT SELECT id FROM employees WHERE department_id IN (1, 2);

-- Query 64: EXCEPT
SELECT id FROM employees EXCEPT SELECT employee_id FROM employee_projects;


-- ============================================================================
-- SECTION 9: ORDER BY & Sorting (4 queries)
-- ============================================================================

-- Query 65: Simple ORDER BY ascending
SELECT name, salary FROM employees ORDER BY salary ASC;

-- Query 66: ORDER BY descending
SELECT name, hire_date FROM employees ORDER BY hire_date DESC;

-- Query 67: ORDER BY multiple columns
SELECT name, department_id, salary FROM employees ORDER BY department_id ASC, salary DESC;

-- Query 68: ORDER BY with NULL handling
SELECT name, manager_id FROM employees ORDER BY manager_id NULLS FIRST;


-- ============================================================================
-- SECTION 10: DDL Operations (10 queries)
-- ============================================================================

-- Query 69: CREATE TABLE with PRIMARY KEY
CREATE TABLE test_customers (customer_id INTEGER PRIMARY KEY, customer_name VARCHAR(100) NOT NULL, email VARCHAR(100));

-- Query 70: CREATE TABLE with FOREIGN KEY
CREATE TABLE test_orders (order_id INTEGER PRIMARY KEY, customer_id INTEGER, order_date DATE, FOREIGN KEY (customer_id) REFERENCES test_customers(customer_id));

-- Query 71: CREATE INDEX
CREATE INDEX idx_employees_department ON employees(department_id);

-- Query 72: CREATE UNIQUE INDEX
CREATE UNIQUE INDEX idx_employees_email ON employees(email);

-- Query 73: CREATE VIEW
CREATE VIEW active_employees AS SELECT id, name, department_id, salary FROM employees WHERE is_active = TRUE;

-- Query 74: CREATE MATERIALIZED VIEW
CREATE MATERIALIZED VIEW department_summary AS SELECT d.id, d.name, COUNT(e.id) AS employee_count, AVG(e.salary) AS avg_salary FROM departments d LEFT JOIN employees e ON d.id = e.department_id GROUP BY d.id, d.name;

-- Query 75: ALTER TABLE ADD COLUMN
ALTER TABLE employees ADD COLUMN phone VARCHAR(20);

-- Query 76: ALTER TABLE DROP COLUMN
ALTER TABLE employees DROP COLUMN phone;

-- Query 77: ALTER TABLE RENAME COLUMN
ALTER TABLE test_customers RENAME COLUMN customer_name TO full_name;

-- Query 78: DROP TABLE
DROP TABLE IF EXISTS test_orders;

-- Query 78b: DROP TABLE (second part)
DROP TABLE IF EXISTS test_customers;


-- ============================================================================
-- SECTION 11: Data Types Testing (8 queries)
-- ============================================================================

-- Query 79: Integer types (SMALLINT, INTEGER, BIGINT)
CREATE TABLE test_integers (id INTEGER PRIMARY KEY, small_val SMALLINT, int_val INTEGER, big_val BIGINT);

-- Query 79b: Insert test integers
INSERT INTO test_integers VALUES (1, 100, 50000, 9223372036854775807);

-- Query 79c: Select test integers
SELECT * FROM test_integers;

-- Query 80: Floating point types (REAL, DOUBLE PRECISION)
CREATE TABLE test_floats (id INTEGER PRIMARY KEY, real_val REAL, double_val DOUBLE PRECISION);

-- Query 80b: Insert test floats
INSERT INTO test_floats VALUES (1, 3.14, 3.14159265359);

-- Query 80c: Select test floats
SELECT * FROM test_floats;

-- Query 81: NUMERIC/DECIMAL with precision
CREATE TABLE test_numeric (id INTEGER PRIMARY KEY, price NUMERIC(10, 2), rate DECIMAL(5, 4));

-- Query 81b: Insert test numeric
INSERT INTO test_numeric VALUES (1, 12345.67, 0.9875);

-- Query 81c: Select test numeric
SELECT * FROM test_numeric;

-- Query 82: Character types (CHAR, VARCHAR, TEXT)
CREATE TABLE test_strings (id INTEGER PRIMARY KEY, fixed_char CHAR(10), var_char VARCHAR(100), text_val TEXT);

-- Query 82b: Insert test strings
INSERT INTO test_strings VALUES (1, 'ABC', 'Variable length string', 'This is a long text value that can be of any length');

-- Query 82c: Select test strings
SELECT * FROM test_strings;

-- Query 83: DATE and TIMESTAMP types
CREATE TABLE test_dates (id INTEGER PRIMARY KEY, event_date DATE, event_timestamp TIMESTAMP, event_time TIME);

-- Query 83b: Insert test dates
INSERT INTO test_dates VALUES (1, '2023-06-15', '2023-06-15 14:30:00', '14:30:00');

-- Query 83c: Select test dates
SELECT * FROM test_dates;

-- Query 84: BOOLEAN type
CREATE TABLE test_boolean (id INTEGER PRIMARY KEY, is_active BOOLEAN, is_deleted BOOLEAN DEFAULT FALSE);

-- Query 84b: Insert test boolean
INSERT INTO test_boolean (id, is_active) VALUES (1, TRUE), (2, FALSE);

-- Query 84c: Select test boolean
SELECT * FROM test_boolean;

-- Query 85: UUID type
CREATE TABLE test_uuid (id UUID PRIMARY KEY, name VARCHAR(100));

-- Query 86: Array type
CREATE TABLE test_arrays (id INTEGER PRIMARY KEY, tags TEXT[], numbers INTEGER[]);


-- ============================================================================
-- SECTION 12: Transactions (4 queries)
-- ============================================================================

-- Query 87: Basic transaction with COMMIT
BEGIN;

-- Query 87b: Insert in transaction
INSERT INTO departments (id, name, budget, location) VALUES (100, 'Research', 600000.00, 'Boston');

-- Query 87c: Update in transaction
UPDATE departments SET budget = 650000.00 WHERE id = 100;

-- Query 87d: Commit transaction
COMMIT;

-- Query 88: Transaction with ROLLBACK
BEGIN;

-- Query 88b: Delete in transaction
DELETE FROM employees WHERE id = 1;

-- Query 88c: Rollback transaction
ROLLBACK;

-- Query 89: Multiple operations in transaction
BEGIN;

-- Query 89b: Update in transaction
UPDATE employees SET salary = salary * 1.05 WHERE department_id = 1;

-- Query 89c: Insert in transaction
INSERT INTO salaries_history (id, employee_id, salary, effective_date) SELECT id + 100, id, salary, '2023-01-01' FROM employees WHERE department_id = 1;

-- Query 89d: Commit transaction
COMMIT;

-- Query 90: Read within transaction
BEGIN;

-- Query 90b: Select in transaction
SELECT * FROM employees WHERE id = 1;

-- Query 90c: Update in transaction
UPDATE employees SET salary = 96000.00 WHERE id = 1;

-- Query 90d: Select in transaction
SELECT * FROM employees WHERE id = 1;

-- Query 90e: Commit transaction
COMMIT;


-- ============================================================================
-- SECTION 13: Complex Multi-Feature Queries (10 queries)
-- ============================================================================

-- Query 91: CTE + Window Functions + Aggregation
WITH dept_salaries AS (SELECT department_id, salary, AVG(salary) OVER (PARTITION BY department_id) AS dept_avg, RANK() OVER (PARTITION BY department_id ORDER BY salary DESC) AS dept_rank FROM employees) SELECT department_id, COUNT(*) AS employees_above_avg FROM dept_salaries WHERE salary > dept_avg GROUP BY department_id;

-- Query 92: Multiple JOINs + Subquery + Aggregation
SELECT d.name AS department, COUNT(DISTINCT e.id) AS employee_count, COUNT(DISTINCT p.id) AS project_count, SUM(ep.hours_allocated) AS total_hours FROM departments d LEFT JOIN employees e ON d.id = e.department_id LEFT JOIN employee_projects ep ON e.id = ep.employee_id LEFT JOIN projects p ON ep.project_id = p.id WHERE d.budget > (SELECT AVG(budget) FROM departments) GROUP BY d.name HAVING COUNT(DISTINCT e.id) > 0;

-- Query 93: Recursive CTE + Window Functions
WITH RECURSIVE emp_tree AS (SELECT id, name, manager_id, salary, 0 AS level, CAST(name AS VARCHAR(1000)) AS path FROM employees WHERE manager_id IS NULL UNION ALL SELECT e.id, e.name, e.manager_id, e.salary, et.level + 1, CAST(et.path || ' -> ' || e.name AS VARCHAR(1000)) FROM employees e JOIN emp_tree et ON e.manager_id = et.id) SELECT name, level, path, salary, AVG(salary) OVER (PARTITION BY level) AS avg_salary_at_level FROM emp_tree ORDER BY level, name;

-- Query 94: Complex subquery with multiple aggregations
SELECT e.name, e.salary, (SELECT AVG(salary) FROM employees WHERE department_id = e.department_id) AS dept_avg, (SELECT MAX(salary) FROM employees) AS company_max, CASE WHEN e.salary > (SELECT AVG(salary) FROM employees WHERE department_id = e.department_id) THEN 'Above Average' ELSE 'Below Average' END AS performance FROM employees e WHERE e.is_active = TRUE;

-- Query 95: Multiple CTEs with JOINs and window functions
WITH dept_stats AS (SELECT department_id, AVG(salary) AS avg_salary, COUNT(*) AS emp_count FROM employees GROUP BY department_id), ranked_depts AS (SELECT d.id, d.name, ds.avg_salary, ds.emp_count, RANK() OVER (ORDER BY ds.avg_salary DESC) AS salary_rank FROM departments d JOIN dept_stats ds ON d.id = ds.department_id) SELECT rd.name, rd.avg_salary, rd.emp_count, rd.salary_rank, e.name AS employee_name, e.salary FROM ranked_depts rd JOIN employees e ON e.department_id = rd.id WHERE rd.salary_rank <= 3 ORDER BY rd.salary_rank, e.salary DESC;

-- Query 96: Set operations with aggregations
SELECT 'High Earners' AS category, COUNT(*) AS count FROM employees WHERE salary > 80000.00 UNION ALL SELECT 'Low Earners' AS category, COUNT(*) AS count FROM employees WHERE salary <= 60000.00 UNION ALL SELECT 'Mid Range' AS category, COUNT(*) AS count FROM employees WHERE salary > 60000.00 AND salary <= 80000.00;

-- Query 97: Correlated subqueries with EXISTS
SELECT d.name AS department, (SELECT COUNT(*) FROM employees e WHERE e.department_id = d.id) AS emp_count, d.budget FROM departments d WHERE EXISTS (SELECT 1 FROM employees e WHERE e.department_id = d.id AND e.salary > (SELECT AVG(salary) FROM employees WHERE department_id = d.id));

-- Query 98: Complex JOIN with multiple conditions and aggregations
SELECT e.name AS employee, d.name AS department, COUNT(DISTINCT p.id) AS project_count, SUM(ep.hours_allocated) AS total_hours, AVG(p.budget) AS avg_project_budget FROM employees e JOIN departments d ON e.department_id = d.id LEFT JOIN employee_projects ep ON e.id = ep.employee_id LEFT JOIN projects p ON ep.project_id = p.id AND p.start_date >= '2023-01-01' WHERE e.is_active = TRUE GROUP BY e.name, d.name HAVING COUNT(DISTINCT p.id) > 0 ORDER BY total_hours DESC;

-- Query 99: Window functions with complex partitioning
SELECT e.name, e.department_id, e.salary, e.hire_date, ROW_NUMBER() OVER (PARTITION BY e.department_id ORDER BY e.hire_date) AS dept_hire_order, RANK() OVER (PARTITION BY e.department_id ORDER BY e.salary DESC) AS dept_salary_rank, SUM(e.salary) OVER (PARTITION BY e.department_id) AS dept_total_salary, AVG(e.salary) OVER (PARTITION BY e.department_id) AS dept_avg_salary, e.salary - AVG(e.salary) OVER (PARTITION BY e.department_id) AS salary_vs_dept_avg FROM employees e WHERE e.is_active = TRUE ORDER BY e.department_id, e.salary DESC;

-- Query 100: Ultimate complex query - CTEs, window functions, subqueries, joins, aggregations
WITH project_stats AS (SELECT p.id, p.name, p.department_id, COUNT(ep.employee_id) AS team_size, SUM(ep.hours_allocated) AS total_hours, p.budget / NULLIF(COUNT(ep.employee_id), 0) AS budget_per_person FROM projects p LEFT JOIN employee_projects ep ON p.id = ep.project_id GROUP BY p.id, p.name, p.department_id, p.budget), dept_performance AS (SELECT d.id, d.name, d.budget, COUNT(DISTINCT e.id) AS employee_count, AVG(e.salary) AS avg_salary, COUNT(DISTINCT ps.id) AS project_count, SUM(ps.total_hours) AS total_project_hours FROM departments d LEFT JOIN employees e ON d.id = e.department_id AND e.is_active = TRUE LEFT JOIN project_stats ps ON d.id = ps.department_id GROUP BY d.id, d.name, d.budget), ranked_departments AS (SELECT *, RANK() OVER (ORDER BY avg_salary DESC) AS salary_rank, RANK() OVER (ORDER BY employee_count DESC) AS size_rank, RANK() OVER (ORDER BY project_count DESC) AS project_rank, budget / NULLIF(employee_count, 0) AS budget_per_employee FROM dept_performance) SELECT rd.name AS department, rd.employee_count, rd.project_count, rd.avg_salary, rd.budget_per_employee, rd.salary_rank, rd.size_rank, rd.project_rank, CASE WHEN rd.salary_rank <= 2 AND rd.project_rank <= 2 THEN 'Top Performer' WHEN rd.salary_rank <= 3 OR rd.project_rank <= 3 THEN 'Good Performer' ELSE 'Average Performer' END AS performance_category, (SELECT COUNT(*) FROM employees e WHERE e.department_id = rd.id AND e.salary > rd.avg_salary) AS above_avg_employees FROM ranked_departments rd WHERE rd.employee_count > 0 ORDER BY CASE WHEN rd.salary_rank <= 2 AND rd.project_rank <= 2 THEN 1 WHEN rd.salary_rank <= 3 OR rd.project_rank <= 3 THEN 2 ELSE 3 END, rd.avg_salary DESC;


-- ============================================================================
-- END OF COMPREHENSIVE TEST QUERY SUITE
-- ============================================================================
-- Summary:
-- - 100+ comprehensive test queries (including sub-statements) covering all major SQL features
-- - All queries written on single lines for easy parsing
-- - Organized by feature category for easy navigation
-- - Tests basic operations through complex multi-feature queries
-- - Includes DDL, DML, aggregations, joins, subqueries, CTEs, window functions
-- - Covers transaction handling and various data types
-- - Demonstrates the full capabilities of RustgreSQL
-- ============================================================================
