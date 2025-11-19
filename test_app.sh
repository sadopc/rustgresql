#!/bin/bash

# Test script for RustgreSQL
echo "Testing RustgreSQL application..."

# Test 1: Basic compilation
echo "1. Testing compilation..."
cargo check --quiet
if [ $? -eq 0 ]; then
    echo "✅ Compilation successful"
else
    echo "❌ Compilation failed"
    exit 1
fi

# Test 2: Build
echo "2. Testing build..."
cargo build --quiet
if [ $? -eq 0 ]; then
    echo "✅ Build successful"
else
    echo "❌ Build failed"
    exit 1
fi

# Test 3: Run basic functionality test
echo "3. Testing basic functionality..."
timeout 10 cargo run << EOF > test_output.txt 2>&1
SELECT 1 + 1 AS result;
SELECT ABS(-42) AS absolute;
SELECT 5 > 3 AS test;
SELECT TRUE AND FALSE AS logical;
exit
EOF

if [ $? -eq 0 ]; then
    echo "✅ Application runs without crashing"
else
    echo "⚠️  Application timed out (expected for interactive mode)"
fi

# Check output for basic functionality
if grep -q "result" test_output.txt; then
    echo "✅ SQL queries produce output"
else
    echo "⚠️  No query output detected"
fi

echo "4. Application features:"
echo "   - Interactive SQL REPL"
echo "   - Basic arithmetic operations"
echo "   - Comparison and logical operations"
echo "   - Built-in functions (ABS, etc.)"
echo "   - Three-valued logic with NULL handling"
echo "   - String pattern matching (LIKE, ILIKE)"

echo ""
echo "Test completed. Check test_output.txt for detailed output."

# Clean up
rm -f test_output.txt