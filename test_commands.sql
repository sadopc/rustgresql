-- Test file for RustgreSQL
-- Create a simple users table
CREATE TABLE users (
    id INTEGER,
    name VARCHAR(100),
    email VARCHAR(100),
    age INTEGER,
    active BOOLEAN
);

-- Insert some test data
INSERT INTO users (id, name, email, age, active) VALUES (1, 'Alice Johnson', 'alice@example.com', 28, true);
INSERT INTO users (id, name, email, age, active) VALUES (2, 'Bob Smith', 'bob@example.com', 35, true);
INSERT INTO users (id, name, email, age, active) VALUES (3, 'Charlie Brown', 'charlie@example.com', 42, false);
INSERT INTO users (id, name, email, age, active) VALUES (4, 'Diana Prince', 'diana@example.com', 30, true);
INSERT INTO users (id, name, email, age, active) VALUES (5, 'Eve Davis', 'eve@example.com', 25, false);

-- Select all users
SELECT * FROM users;

-- Select specific columns
SELECT name, age FROM users;

-- Select with WHERE clause
SELECT name, email FROM users WHERE active = true;

-- Select with age filter
SELECT name, age FROM users WHERE age > 30;

-- Count active users
SELECT COUNT(*) FROM users WHERE active = true;
