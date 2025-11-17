#!/bin/bash

# Test script for RustgreSQL fixes

echo "Testing RustgreSQL fixes..."

# Clean up any existing data
rm -f auto_increment_counters.json

# Start the database in the background
./target/release/rustgresql &
DB_PID=$!

# Wait for it to start
sleep 2

# Send commands via a here document
{
    echo "CREATE TABLE users (id SERIAL PRIMARY KEY, name VARCHAR(100) NOT NULL, email TEXT UNIQUE, age INTEGER CHECK (age >= 18), created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP);"
    sleep 1
    echo "CREATE TABLE products (product_id INTEGER PRIMARY KEY, name VARCHAR(255) NOT NULL, price NUMERIC(10,2) CHECK (price > 0), category VARCHAR(50), in_stock BOOLEAN DEFAULT true);"
    sleep 1
    echo "CREATE TABLE orders (order_id SERIAL PRIMARY KEY, user_id INTEGER REFERENCES users(id), product_id INTEGER REFERENCES products(product_id), quantity INTEGER NOT NULL CHECK (quantity > 0), order_date DATE DEFAULT CURRENT_DATE);"
    sleep 1
    echo "CREATE INDEX idx_users_email ON users(email);"
    sleep 1
    echo "CREATE INDEX idx_products_category ON products(category);"
    sleep 1
    echo "INSERT INTO users (name, email, age) VALUES ('Alice Johnson', 'alice@example.com', 25);"
    sleep 1
    echo "INSERT INTO users (name, email, age) VALUES ('Bob Smith', 'bob@example.com', 30);"
    sleep 1
    echo "INSERT INTO users (name, email, age) VALUES ('Charlie Brown', 'charlie@example.com', 35);"
    sleep 1
    echo "INSERT INTO products (product_id, name, price, category) VALUES (1, 'Laptop', 999.99, 'Electronics');"
    sleep 1
    echo "INSERT INTO products (product_id, name, price, category) VALUES (2, 'Mouse', 29.99, 'Electronics');"
    sleep 1
    echo "SELECT * FROM users;"
    sleep 1
    echo "SELECT * FROM products;"
    sleep 1
    echo "exit"
} | telnet localhost 5432 2>/dev/null || {
    echo "Telnet not available, trying netcat..."
    {
        echo "CREATE TABLE users (id SERIAL PRIMARY KEY, name VARCHAR(100) NOT NULL, email TEXT UNIQUE, age INTEGER CHECK (age >= 18), created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP);"
        sleep 1
        echo "CREATE TABLE products (product_id INTEGER PRIMARY KEY, name VARCHAR(255) NOT NULL, price NUMERIC(10,2) CHECK (price > 0), category VARCHAR(50), in_stock BOOLEAN DEFAULT true);"
        sleep 1
        echo "CREATE TABLE orders (order_id SERIAL PRIMARY KEY, user_id INTEGER REFERENCES users(id), product_id INTEGER REFERENCES products(product_id), quantity INTEGER NOT NULL CHECK (quantity > 0), order_date DATE DEFAULT CURRENT_DATE);"
        sleep 1
        echo "CREATE INDEX idx_users_email ON users(email);"
        sleep 1
        echo "CREATE INDEX idx_products_category ON products(category);"
        sleep 1
        echo "INSERT INTO users (name, email, age) VALUES ('Alice Johnson', 'alice@example.com', 25);"
        sleep 1
        echo "INSERT INTO users (name, email, age) VALUES ('Bob Smith', 'bob@example.com', 30);"
        sleep 1
        echo "INSERT INTO users (name, email, age) VALUES ('Charlie Brown', 'charlie@example.com', 35);"
        sleep 1
        echo "INSERT INTO products (product_id, name, price, category) VALUES (1, 'Laptop', 999.99, 'Electronics');"
        sleep 1
        echo "INSERT INTO products (product_id, name, price, category) VALUES (2, 'Mouse', 29.99, 'Electronics');"
        sleep 1
        echo "SELECT * FROM users;"
        sleep 1
        echo "SELECT * FROM products;"
        sleep 1
        echo "exit"
    } | nc localhost 5432
}

# Kill the database
kill $DB_PID 2>/dev/null
wait $DB_PID 2>/dev/null

echo "Test completed."