# Fix for BOOLEAN DEFAULT Values Bug

## Problem
When a table is created with a BOOLEAN column that has a DEFAULT value, INSERT statements that omit that column were not applying the default value, resulting in NULL instead of the expected default value.

### Example Issue
```sql
CREATE TABLE test_boolean (
    id INTEGER PRIMARY KEY,
    is_active BOOLEAN,
    is_deleted BOOLEAN DEFAULT FALSE
);

INSERT INTO test_boolean (id, is_active) VALUES (1, TRUE);

-- Expected: is_deleted = FALSE
-- Actual: is_deleted = NULL
```

## Root Cause
The `InsertOperator::map_values_to_table_schema()` method in `src/executor/operators.rs` had a TODO comment at line 1004 indicating that DEFAULT value handling was not implemented. The code only handled SERIAL auto-increment columns but did not apply default values for other column types.

## Solution
Modified `/mnt/c/Coding/rustgresql/src/executor/operators.rs` to apply default values when:
1. A column value is NULL
2. The column is not a SERIAL type
3. The column has a default value defined

### Code Change
```rust
// Handle auto-increment columns (SERIAL) and DEFAULT values
for (i, column) in table_def.columns.iter().enumerate() {
    if full_row[i].kind == ValueKind::Null(crate::types::NullValue) {
        // Check if this is a SERIAL column
        if column.data_type.kind == crate::types::DataTypeKind::Serial {
            // Generate auto-increment value (simplified - should use proper sequence)
            let next_id = self.generate_auto_increment_value(scanner, &column.name)?;
            full_row[i] = Value { kind: ValueKind::Integer(next_id) };
        } else if let Some(ref default_value) = column.default_value {
            // Apply DEFAULT value if column has one
            full_row[i] = default_value.clone();
        }
    }
}
```

## Test Coverage
Added comprehensive test in `/mnt/c/Coding/rustgresql/src/executor/ddl_tests.rs`:
- Test function: `test_boolean_default_values()`
- Tests CREATE TABLE with BOOLEAN DEFAULT FALSE
- Tests INSERT that omits the column with DEFAULT
- Verifies the default value is correctly applied

## Files Modified
1. `/mnt/c/Coding/rustgresql/src/executor/operators.rs` - Implemented DEFAULT value handling
2. `/mnt/c/Coding/rustgresql/src/executor/ddl_tests.rs` - Added test for BOOLEAN DEFAULT values
3. `/mnt/c/Coding/rustgresql/tests/test_query_optimizer_limit.rs` - Fixed TableRef enum usage
4. `/mnt/c/Coding/rustgresql/src/executor/tests/window_functions.rs` - Fixed SelectItem to ColumnSpec usage

## Verification
The fix ensures that:
- BOOLEAN DEFAULT values are correctly applied during INSERT
- Other data types with DEFAULT values also work (INTEGER, TEXT, etc.)
- No regressions to existing SERIAL auto-increment functionality
