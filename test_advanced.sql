-- ==================================================
-- RustgreSQL Advanced Test Suite
-- ==================================================

-- Test 1: Create a products table
-- ==================================================
CREATE TABLE products (
    product_id INTEGER,
    product_name VARCHAR(100),
    category VARCHAR(50),
    price FLOAT,
    in_stock BOOLEAN
);

-- Test 2: Insert products
-- ==================================================
INSERT INTO products VALUES (1, 'Laptop', 'Electronics', 999.99, true);
INSERT INTO products VALUES (2, 'Mouse', 'Electronics', 29.99, true);
INSERT INTO products VALUES (3, 'Desk Chair', 'Furniture', 199.99, false);
INSERT INTO products VALUES (4, 'Monitor', 'Electronics', 299.99, true);
INSERT INTO products VALUES (5, 'Keyboard', 'Electronics', 79.99, true);
INSERT INTO products VALUES (6, 'Bookshelf', 'Furniture', 149.99, true);

-- Test 3: Select with price range
-- ==================================================
SELECT product_name, price FROM products WHERE price > 50 AND price < 300;

-- Test 4: Select by category and stock status
-- ==================================================
SELECT product_name, price FROM products WHERE category = 'Electronics' AND in_stock = true;

-- Test 5: Create orders table
-- ==================================================
CREATE TABLE orders (
    order_id INTEGER,
    product_id INTEGER,
    quantity INTEGER,
    order_date VARCHAR(50)
);

-- Test 6: Insert orders
-- ==================================================
INSERT INTO orders VALUES (1, 1, 2, '2024-01-15');
INSERT INTO orders VALUES (2, 2, 5, '2024-01-16');
INSERT INTO orders VALUES (3, 4, 1, '2024-01-17');
INSERT INTO orders VALUES (4, 1, 1, '2024-01-18');
INSERT INTO orders VALUES (5, 5, 3, '2024-01-19');

-- Test 7: Select all orders
-- ==================================================
SELECT * FROM orders;

-- Test 8: Count orders for a specific product
-- ==================================================
SELECT COUNT(*) FROM orders WHERE product_id = 1;

-- Test 9: Update product price
-- ==================================================
UPDATE products SET price = 899.99 WHERE product_id = 1;

-- Test 10: Verify price update
-- ==================================================
SELECT product_name, price FROM products WHERE product_id = 1;

-- Test 11: Multiple updates
-- ==================================================
UPDATE products SET in_stock = false WHERE category = 'Furniture';

-- Test 12: Verify multiple updates
-- ==================================================
SELECT product_name, in_stock FROM products WHERE category = 'Furniture';

-- Test 13: Delete out of stock items
-- ==================================================
DELETE FROM products WHERE in_stock = false;

-- Test 14: Check remaining products
-- ==================================================
SELECT COUNT(*) FROM products;
SELECT * FROM products;
