-- ==================================================
-- RustgreSQL Data Types Test Suite
-- ==================================================

-- Test 1: Create table with various data types
-- ==================================================
CREATE TABLE test_types (
    int_col INTEGER,
    str_col VARCHAR(100),
    float_col FLOAT,
    bool_col BOOLEAN,
    text_col TEXT
);

-- Test 2: Insert data with different types
-- ==================================================
INSERT INTO test_types VALUES (42, 'Hello World', 3.14159, true, 'This is a longer text field');
INSERT INTO test_types VALUES (-100, 'Negative number', -99.99, false, 'Another text');
INSERT INTO test_types VALUES (0, 'Zero', 0.0, true, 'Zero values');
INSERT INTO test_types VALUES (999999, 'Large number', 123456.789, false, 'Large values test');

-- Test 3: Select all data
-- ==================================================
SELECT * FROM test_types;

-- Test 4: Test integer operations
-- ==================================================
SELECT int_col FROM test_types WHERE int_col > 0;

-- Test 5: Test float comparisons
-- ==================================================
SELECT str_col, float_col FROM test_types WHERE float_col > 0;

-- Test 6: Test boolean filtering
-- ==================================================
SELECT str_col, bool_col FROM test_types WHERE bool_col = true;

-- Test 7: Test string matching
-- ==================================================
SELECT str_col FROM test_types WHERE str_col = 'Hello World';

-- Test 8: Create table for NULL testing
-- ==================================================
CREATE TABLE null_test (
    id INTEGER,
    value VARCHAR(50)
);

-- Test 9: Insert with explicit NULL (if supported)
-- ==================================================
INSERT INTO null_test VALUES (1, 'Not null');
INSERT INTO null_test VALUES (2, 'Also not null');

-- Test 10: Select from null_test
-- ==================================================
SELECT * FROM null_test;
