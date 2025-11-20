-- Test a simple subquery without aggregation
SELECT name, (SELECT name FROM employees WHERE id = 1) AS first_employee_name FROM employees LIMIT 3;