-- 1. Setup: Create Departments Table
CREATE TABLE departments (
    department_id INTEGER PRIMARY KEY,
    department_name VARCHAR(100) NOT NULL,
    location VARCHAR(100)
);

-- 2. Setup: Create Employees Table
CREATE TABLE employees (
    employee_id INTEGER PRIMARY KEY,
    first_name VARCHAR(50) NOT NULL,
    last_name VARCHAR(50) NOT NULL,
    email VARCHAR(100) UNIQUE,
    phone_number VARCHAR(20),
    hire_date DATE NOT NULL,
    job_id VARCHAR(10) NOT NULL,
    salary DECIMAL(10, 2),
    department_id INTEGER REFERENCES departments(department_id)
);

-- 3. Setup: Create Projects Table
CREATE TABLE projects (
    project_id INTEGER PRIMARY KEY,
    project_name VARCHAR(100) NOT NULL,
    start_date DATE,
    end_date DATE,
    budget DECIMAL(12, 2)
);

-- 4. Setup: Create Employee_Projects Table
CREATE TABLE employee_projects (
    employee_id INTEGER REFERENCES employees(employee_id),
    project_id INTEGER REFERENCES projects(project_id),
    hours_worked DECIMAL(5, 2),
    PRIMARY KEY (employee_id, project_id)
);

-- 5. Setup: Create Products Table
CREATE TABLE products (
    product_id INTEGER PRIMARY KEY,
    product_name VARCHAR(100) NOT NULL,
    category_id INTEGER,
    unit_price DECIMAL(10, 2),
    units_in_stock INTEGER
);

-- 6. Setup: Create Categories Table
CREATE TABLE categories (
    category_id INTEGER PRIMARY KEY,
    category_name VARCHAR(50) NOT NULL,
    description TEXT
);

-- 7. Setup: Create Customers Table
CREATE TABLE customers (
    customer_id INTEGER PRIMARY KEY,
    company_name VARCHAR(100),
    contact_name VARCHAR(50),
    city VARCHAR(50),
    country VARCHAR(50)
);

-- 8. Setup: Create Orders Table
CREATE TABLE orders (
    order_id INTEGER PRIMARY KEY,
    customer_id INTEGER REFERENCES customers(customer_id),
    employee_id INTEGER REFERENCES employees(employee_id),
    order_date DATE,
    ship_city VARCHAR(50),
    ship_country VARCHAR(50)
);

-- 9. Setup: Create Order_Items Table
CREATE TABLE order_items (
    order_id INTEGER REFERENCES orders(order_id),
    product_id INTEGER REFERENCES products(product_id),
    unit_price DECIMAL(10, 2) NOT NULL,
    quantity INTEGER NOT NULL,
    discount REAL,
    PRIMARY KEY (order_id, product_id)
);

-- 10. Setup: Create Index on Employees
CREATE INDEX idx_emp_last_name ON employees(last_name);

-- 11. Setup: Create Index on Orders
CREATE INDEX idx_orders_cust_id ON orders(customer_id);

-- 12. Setup: Alter Table
ALTER TABLE employees ADD COLUMN manager_id INTEGER;

-- 13. Setup: Create View
CREATE VIEW high_value_orders AS
SELECT order_id, customer_id, order_date
FROM orders
WHERE order_id > 1000;

-- 14. Setup: Create Materialized View
CREATE MATERIALIZED VIEW product_summary AS
SELECT category_id, COUNT(*) as product_count, AVG(unit_price) as avg_price
FROM products
GROUP BY category_id;

-- 15. Setup: Refresh Materialized View
REFRESH MATERIALIZED VIEW product_summary;

-- 16. Insert: Departments
INSERT INTO departments (department_id, department_name, location) VALUES (1, 'IT', 'New York');
INSERT INTO departments (department_id, department_name, location) VALUES (2, 'Sales', 'London');
INSERT INTO departments (department_id, department_name, location) VALUES (3, 'HR', 'Paris');

-- 17. Insert: Employees
INSERT INTO employees (employee_id, first_name, last_name, email, hire_date, job_id, salary, department_id) 
VALUES (101, 'John', 'Doe', 'john.doe@example.com', '2023-01-15', 'IT_PROG', 6000.00, 1);

-- 18. Insert: Employees
INSERT INTO employees (employee_id, first_name, last_name, email, hire_date, job_id, salary, department_id) 
VALUES (102, 'Jane', 'Smith', 'jane.smith@example.com', '2023-02-20', 'SA_REP', 8000.00, 2);

-- 19. Insert: Employees
INSERT INTO employees (employee_id, first_name, last_name, email, hire_date, job_id, salary, department_id) 
VALUES (103, 'Mike', 'Johnson', 'mike.j@example.com', '2023-03-10', 'IT_MGR', 12000.00, 1);

-- 20. Insert: Projects
INSERT INTO projects (project_id, project_name, start_date, budget) VALUES (1, 'Project Alpha', '2024-01-01', 50000.00);
INSERT INTO projects (project_id, project_name, start_date, budget) VALUES (2, 'Project Beta', '2024-02-01', 75000.00);

-- 21. Insert: Employee Projects
INSERT INTO employee_projects (employee_id, project_id, hours_worked) VALUES (101, 1, 40.5);
INSERT INTO employee_projects (employee_id, project_id, hours_worked) VALUES (102, 2, 32.0);

-- 22. Insert: Categories
INSERT INTO categories (category_id, category_name) VALUES (1, 'Electronics');
INSERT INTO categories (category_id, category_name) VALUES (2, 'Books');

-- 23. Insert: Products
INSERT INTO products (product_id, product_name, category_id, unit_price, units_in_stock) VALUES (1, 'Laptop', 1, 1200.00, 50);
INSERT INTO products (product_id, product_name, category_id, unit_price, units_in_stock) VALUES (2, 'Smartphone', 1, 800.00, 100);
INSERT INTO products (product_id, product_name, category_id, unit_price, units_in_stock) VALUES (3, 'Novel', 2, 15.00, 200);

-- 24. Insert: Customers
INSERT INTO customers (customer_id, company_name, city, country) VALUES (1, 'TechCorp', 'Berlin', 'Germany');
INSERT INTO customers (customer_id, company_name, city, country) VALUES (2, 'BookStore', 'London', 'UK');

-- 25. Insert: Orders
INSERT INTO orders (order_id, customer_id, employee_id, order_date) VALUES (1001, 1, 101, '2024-01-05');
INSERT INTO orders (order_id, customer_id, employee_id, order_date) VALUES (1002, 2, 102, '2024-01-06');

-- 26. Insert: Order Items
INSERT INTO order_items (order_id, product_id, unit_price, quantity, discount) VALUES (1001, 1, 1200.00, 2, 0.0);
INSERT INTO order_items (order_id, product_id, unit_price, quantity, discount) VALUES (1001, 2, 800.00, 1, 0.05);

-- 27. Insert: More Employees for Aggregate Tests
INSERT INTO employees (employee_id, first_name, last_name, email, hire_date, job_id, salary, department_id) 
VALUES (104, 'Sarah', 'Connor', 'sarah.c@example.com', '2023-04-01', 'HR_REP', 5000.00, 3);

-- 28. Insert: More Products
INSERT INTO products (product_id, product_name, category_id, unit_price, units_in_stock) VALUES (4, 'Tablet', 1, 300.00, 150);

-- 29. Insert: More Orders
INSERT INTO orders (order_id, customer_id, employee_id, order_date) VALUES (1003, 1, 101, '2024-02-15');

-- 30. Insert: Order Items
INSERT INTO order_items (order_id, product_id, unit_price, quantity, discount) VALUES (1003, 4, 300.00, 5, 0.1);

-- 31. Update: Simple Update
UPDATE employees SET salary = salary + 500 WHERE department_id = 1;

-- 32. Update: Multiple Fields
UPDATE products SET unit_price = 1100.00, units_in_stock = 45 WHERE product_id = 1;

-- 33. Delete: Single Row
DELETE FROM order_items WHERE order_id = 1001 AND product_id = 2;

-- 34. Delete: With Condition
DELETE FROM employees WHERE salary < 3000;

-- 35. Select: All Columns
SELECT * FROM employees;

-- 36. Select: Specific Columns
SELECT first_name, last_name, salary FROM employees;

-- 37. Select: With Alias
SELECT first_name AS name, salary * 12 AS annual_salary FROM employees;

-- 38. Select: Distinct
SELECT DISTINCT department_id FROM employees;

-- 39. Select: Calculation
SELECT unit_price * quantity AS line_total FROM order_items;

-- 40. Select: String Concatenation (assuming + works as noted in parser comments)
SELECT first_name + ' ' + last_name AS full_name FROM employees;

-- 41. Select: Limit
SELECT * FROM products LIMIT 2;

-- 42. Select: Offset
SELECT * FROM products LIMIT 2 OFFSET 1;

-- 43. Select: Order By ASC
SELECT * FROM employees ORDER BY salary;

-- 44. Select: Order By DESC
SELECT * FROM employees ORDER BY salary DESC;

-- 45. Select: Order By Multiple
SELECT * FROM employees ORDER BY department_id, salary DESC;

-- 46. Filtering: Equality
SELECT * FROM products WHERE category_id = 1;

-- 47. Filtering: Greater Than
SELECT * FROM employees WHERE salary > 7000;

-- 48. Filtering: AND
SELECT * FROM employees WHERE department_id = 1 AND salary > 5000;

-- 49. Filtering: OR
SELECT * FROM employees WHERE department_id = 1 OR department_id = 2;

-- 50. Filtering: IN
SELECT * FROM employees WHERE department_id IN (1, 3);

-- 51. Filtering: LIKE
SELECT * FROM employees WHERE first_name LIKE 'J%';

-- 52. Filtering: NOT Equal
SELECT * FROM products WHERE category_id != 1;

-- 53. Filtering: Complex Logic
SELECT * FROM products WHERE (category_id = 1 OR unit_price > 500) AND units_in_stock > 0;

-- 54. Filtering: Less Than or Equal
SELECT * FROM projects WHERE budget <= 60000;

-- 55. Aggregates: Count All
SELECT COUNT(*) FROM employees;

-- 56. Aggregates: Count Column
SELECT COUNT(manager_id) FROM employees;

-- 57. Aggregates: Sum
SELECT SUM(salary) FROM employees;

-- 58. Aggregates: Avg
SELECT AVG(salary) FROM employees;

-- 59. Aggregates: Min
SELECT MIN(salary) FROM employees;

-- 60. Aggregates: Max
SELECT MAX(salary) FROM employees;

-- 61. Group By: Simple
SELECT department_id, COUNT(*) FROM employees GROUP BY department_id;

-- 62. Group By: Multiple Aggregates
SELECT department_id, AVG(salary), SUM(salary) FROM employees GROUP BY department_id;

-- 63. Having
SELECT department_id, AVG(salary) FROM employees GROUP BY department_id HAVING AVG(salary) > 7000;

-- 64. Group By + Order By
SELECT department_id, COUNT(*) FROM employees GROUP BY department_id ORDER BY COUNT(*) DESC;

-- 65. Joins: Inner Join
SELECT e.first_name, d.department_name 
FROM employees e 
INNER JOIN departments d ON e.department_id = d.department_id;

-- 66. Joins: Left Join
SELECT c.company_name, o.order_id 
FROM customers c 
LEFT JOIN orders o ON c.customer_id = o.customer_id;

-- 67. Joins: Right Join
SELECT e.first_name, p.project_name 
FROM employees e 
RIGHT JOIN employee_projects ep ON e.employee_id = ep.employee_id
RIGHT JOIN projects p ON ep.project_id = p.project_id;

-- 68. Joins: Multiple Joins
SELECT o.order_id, c.company_name, e.last_name 
FROM orders o 
INNER JOIN customers c ON o.customer_id = c.customer_id 
INNER JOIN employees e ON o.employee_id = e.employee_id;

-- 69. Joins: Self Join (Manager)
SELECT e.first_name AS emp_name, m.first_name AS mgr_name 
FROM employees e 
LEFT JOIN employees m ON e.manager_id = m.employee_id;

-- 70. Joins: Cross Join (Implicit)
SELECT p.product_name, c.category_name 
FROM products p, categories c 
WHERE p.category_id = c.category_id;

-- 71. Joins: With Alias
SELECT e.last_name, d.department_name 
FROM employees AS e 
JOIN departments AS d ON e.department_id = d.department_id;

-- 72. Subqueries: Scalar in Select
SELECT product_name, (SELECT category_name FROM categories WHERE category_id = products.category_id) AS cat_name 
FROM products;

-- 73. Subqueries: IN Clause
SELECT * FROM employees WHERE department_id IN (SELECT department_id FROM departments WHERE location = 'New York');

-- 74. Subqueries: FROM Clause
SELECT avg_sal FROM (SELECT AVG(salary) as avg_sal FROM employees) as sub;

-- 75. CTE: Simple
WITH DeptCounts AS (
    SELECT department_id, COUNT(*) as emp_count 
    FROM employees 
    GROUP BY department_id
)
SELECT * FROM DeptCounts WHERE emp_count > 1;

-- 76. CTE: Multiple
WITH Sales AS (
    SELECT order_id, SUM(unit_price * quantity) as total 
    FROM order_items 
    GROUP BY order_id
),
TopSales AS (
    SELECT order_id FROM Sales WHERE total > 1000
)
SELECT * FROM orders WHERE order_id IN (SELECT order_id FROM TopSales);

-- 77. Window Functions: Row Number
SELECT first_name, salary, ROW_NUMBER() OVER (ORDER BY salary DESC) as rank_num FROM employees;

-- 78. Window Functions: Rank
SELECT first_name, salary, RANK() OVER (ORDER BY salary DESC) as rank_val FROM employees;

-- 79. Window Functions: Partition By
SELECT first_name, department_id, salary, AVG(salary) OVER (PARTITION BY department_id) as dept_avg FROM employees;

-- 80. Window Functions: Cumulative Sum
SELECT order_id, total_amt, SUM(total_amt) OVER (ORDER BY order_id) as running_total 
FROM (SELECT order_id, SUM(unit_price * quantity) as total_amt FROM order_items GROUP BY order_id) sub;

-- 81. Set Operations: Union
SELECT city FROM customers 
UNION 
SELECT location FROM departments;

-- 82. Set Operations: Union All
SELECT city FROM customers 
UNION ALL 
SELECT location FROM departments;

-- 83. Set Operations: Intersect
SELECT city FROM customers 
INTERSECT 
SELECT location FROM departments; -- May return empty if no match, but syntax test

-- 84. Set Operations: Except
SELECT city FROM customers 
EXCEPT 
SELECT location FROM departments;

-- 85. View: Select from View
SELECT * FROM high_value_orders;

-- 86. Materialized View: Select
SELECT * FROM product_summary;

-- 87. Transaction: Basic (if supported implicitly or explicitly)
-- Since BEGIN/COMMIT might not be supported in parser, we skip explicit transaction control for script
-- But we can test atomic update
UPDATE employees SET salary = salary * 1.05 WHERE department_id = 1;

-- 88. Data Types: Boolean (if supported)
-- Assuming boolean literals TRUE/FALSE
SELECT first_name, (salary > 10000) AS is_highly_paid FROM employees;

-- 89. CASE Statement
SELECT first_name, 
       CASE 
           WHEN salary < 5000 THEN 'Low' 
           WHEN salary < 10000 THEN 'Medium' 
           ELSE 'High' 
       END as salary_grade 
FROM employees;

-- 90. String Function: Length (if supported)
-- SELECT LENGTH(first_name) FROM employees; -- Parser does not seem to have standard functions hardcoded, 
-- but parser.rs lines 1418 supports Count/Sum/etc. Generic functions are parsed as Function call. 
-- Engine might not implement LENGTH. skipping.

-- 91. Select with WHERE True
SELECT * FROM employees WHERE 1=1;

-- 92. Complex Having
SELECT department_id, SUM(salary) 
FROM employees 
GROUP BY department_id 
HAVING SUM(salary) > 10000 AND COUNT(*) > 1;

-- 93. Nested Subqueries
SELECT first_name FROM employees 
WHERE salary > (SELECT AVG(salary) FROM employees WHERE department_id = (SELECT department_id FROM departments WHERE department_name = 'IT'));

-- 94. Cross Join Explicit
SELECT e.first_name, p.product_name 
FROM employees e CROSS JOIN products p LIMIT 5;

-- 95. Drop Index
DROP INDEX idx_emp_last_name;

-- 96. Drop View
DROP VIEW high_value_orders;

-- 97. Drop Table (Cleanup)
DROP TABLE employee_projects;

-- 98. Drop Table
DROP TABLE projects;

-- 99. Recursive CTE (if supported)
WITH RECURSIVE numbers(n) AS (
    SELECT 1
    UNION ALL
    SELECT n + 1 FROM numbers WHERE n < 10
)
SELECT * FROM numbers;

-- 100. Final Count check
SELECT COUNT(*) as total_employees_remaining FROM employees;
