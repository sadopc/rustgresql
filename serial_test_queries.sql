-- Test basic SERIAL functionality
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    email TEXT UNIQUE,
    age INTEGER CHECK (age >= 18),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Test BIGSERIAL functionality
CREATE TABLE logs (
    log_id BIGSERIAL PRIMARY KEY,
    message TEXT,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Test table creation confirmation
SHOW TABLES;

-- Test table structure
DESCRIBE users;
DESCRIBE logs;

-- Test inserting data (this will also test auto-increment if implemented)
INSERT INTO users (name, email, age) VALUES ('John Doe', 'john@example.com', 25);
INSERT INTO users (name, email, age) VALUES ('Jane Smith', 'jane@example.com', 30);

-- Test inserting into BIGSERIAL table
INSERT INTO logs (message) VALUES ('Test log entry 1');
INSERT INTO logs (message) VALUES ('Test log entry 2');

-- Test querying to see if SERIAL/BIGSERIAL values were auto-generated
SELECT * FROM users;
SELECT * FROM logs;

-- Test edge cases
CREATE TABLE test_edge_cases (
    small_id SMALLINT,
    serial_id SERIAL,
    big_id BIGSERIAL,
    normal_int INTEGER,
    text_field TEXT
);

-- Test explicit SERIAL value (should work if supported)
INSERT INTO users (id, name, email, age) VALUES (999, 'Test User', 'test@example.com', 22);

EXIT;