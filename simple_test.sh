#!/bin/bash

# Simple test for the fixes

echo "Testing NUMERIC support..."
echo "CREATE TABLE test_numeric (id INTEGER, price NUMERIC(10,2));" | timeout 10s ./target/release/rustgresql test.db 2>/dev/null | grep -q "Table 'test_numeric' created successfully" && echo "✓ NUMERIC support works" || echo "✗ NUMERIC support failed"

echo "Testing SERIAL persistence..."
rm -f auto_increment_counters.json test.db
echo -e "CREATE TABLE test_serial (id SERIAL PRIMARY KEY, name TEXT);\nINSERT INTO test_serial (name) VALUES ('first');\nINSERT INTO test_serial (name) VALUES ('second');\nSELECT * FROM test_serial;\nexit" | timeout 15s ./target/release/rustgresql test.db 2>/dev/null | grep -A 10 "SELECT \* FROM test_serial" | tail -10

echo "Checking if counters were saved..."
if [ -f "auto_increment_counters.json" ]; then
    echo "✓ SERIAL counters persisted"
    cat auto_increment_counters.json
else
    echo "✗ SERIAL counters not persisted"
fi

echo "Testing CURRENT_TIMESTAMP..."
echo -e "CREATE TABLE test_ts (id INTEGER, created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP);\nINSERT INTO test_ts (id) VALUES (1);\nSELECT * FROM test_ts;\nexit" | timeout 15s ./target/release/rustgresql test.db 2>/dev/null | grep -A 5 "SELECT \* FROM test_ts" | tail -5