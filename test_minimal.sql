-- Test the simplest possible subquery
SELECT (SELECT MAX(id) FROM employees) AS max_id FROM employees LIMIT 1;