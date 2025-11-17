CREATE TABLE users (id SERIAL PRIMARY KEY, name VARCHAR(100), email TEXT UNIQUE NOT NULL, age INTEGER CHECK (age >= 18), created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP);
INSERT INTO users (name, email, age) VALUES ('Alice Johnson', 'alice@example.com', 25);
SELECT * FROM users;
exit
