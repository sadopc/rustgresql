# Window Function Implementation Analysis

## Overview
This document analyzes the window function implementation in rustgresql, focusing on how window functions are parsed, planned, executed, and integrated into expressions. The analysis identifies a potential bug where window functions work standalone but return NULL when used in arithmetic expressions.

## 1. Window Function Parsing

### AST Structure
Window functions are represented in the AST as:
- `Expression::WindowFunction(WindowFunction)` - The window function call
- `WindowFunction` struct contains:
  - `name`: Function name (e.g., "AVG", "SUM")
  - `args`: Function arguments
  - `window_clause`: OVER clause with partition_by, order_by, and frame
  - `window_name`: Optional named window reference

### Location
- **File**: `src/sql/ast.rs` lines 108-114
- **Parser**: `src/sql/parser.rs` - handles OVER clause parsing

## 2. Window Function Planning

### Extract and Replace Strategy
The planner extracts window functions from complex expressions to handle them separately:

```rust
// In planner.rs extract_and_replace_window_functions()
let (extracted_funcs, modified_expr) = self.extract_window_functions_from_expression(
    expr,
    &mut window_func_counter
);
```

### Key Transformation
1. Original expression: `e.salary - AVG(e.salary) OVER (...)`
2. Extract window function with generated name: `win_func_0`
3. Replace with column reference: `e.salary - win_func_0`
4. Create Window node: `window_functions = [("win_func_0", Expression::WindowFunction(...))]`
5. Create Project node: `columns = [("salary_diff", BinaryOp { ..., right: Column("win_func_0") })]`

### Location
- **File**: `src/executor/planner.rs`
- **Method**: `extract_window_functions_from_expression()` (lines 1365-1504)
- **Plan Creation**: `plan_select()` around lines 595-618

## 3. Window Function Execution

### WindowOperator Structure
```rust
pub struct WindowOperator {
    pub input: Box<PlanNode>,
    pub window_functions: Vec<(String, WindowFunction)>,
    pub partition_by: Vec<Expression>,
    pub order_by: Vec<OrderBy>,
    pub window_frame: Option<WindowFrame>,
}
```

### Execution Flow
1. Execute input plan to get source data
2. Sort rows if ORDER BY specified
3. Partition rows if PARTITION BY specified
4. For each partition, initialize window function states
5. Process each row, evaluating window functions
6. Append window function results to each row

### Result Columns
The WindowOperator creates output column names by:
```rust
let mut result_column_names = input_column_names.to_vec();
for (alias, _window_func) in self.window_functions.iter() {
    result_column_names.push(alias.clone());
}
```

So if input has `[e.name, e.salary]` and window function alias is `win_func_0`, output will have `[e.name, e.salary, win_func_0]`.

### Location
- **File**: `src/executor/operators.rs`
- **Struct**: lines 2733-2740
- **Execute Method**: lines 2773-2808
- **Apply Window Functions**: lines 2909-2952

## 4. Integration with Projections

### ProjectOperator Evaluation
The ProjectOperator evaluates projection expressions using an EvaluationContext created from the WindowOperator's output:

```rust
fn create_basic_evaluation_context(&self, column_names: &[String], row: &[Value], context: &ExecutionContext) -> EvaluationContext {
    let mut columns = std::collections::HashMap::new();

    for (i, column_name) in column_names.iter().enumerate() {
        if i < row.len() {
            let value = &row[i];
            columns.insert(column_name.clone(), value.clone());
        }
    }
    // ...
}
```

### Expression Evaluation
When evaluating a BinaryOp like `e.salary - win_func_0`:
1. Evaluate left side: look up "e.salary" or "salary" in context
2. Evaluate right side: look up "win_func_0" in context
3. Perform arithmetic operation

### Location
- **File**: `src/executor/operators.rs`
- **ProjectOperator**: lines 363-461
- **Evaluation Context Creation**: lines 463-541

## 5. Identified Bug

### Problem Statement
Window functions like `AVG()` work fine on their own, but when used in arithmetic expressions like `e.salary - AVG(e.salary) OVER (...)`, the result is NULL.

### Root Cause Analysis

#### Column Lookup Behavior
In `src/executor/expression.rs`, when a column reference is not found, it returns NULL instead of an error:

```rust
Expression::Column { table, name } => {
    // ... various fallbacks ...
    // Not found
    Ok(Value { kind: ValueKind::Null(NullValue) })  // Line 509
}
```

This means if the column lookup fails for `win_func_0`, the result is NULL.

#### Possible Causes
1. **Column Name Mismatch**: The column name in the evaluation context doesn't match the referenced name
2. **Missing Column**: The window function column wasn't added to the result columns
3. **Execution Order Bug**: Window operator hasn't finished when projection runs

### Evidence

The code flow suggests everything should work:
1. Window node creates output columns with alias `win_func_0` ✓
2. Project node receives these columns ✓
3. Project creates context mapping `win_func_0` to value ✓
4. Expression evaluator looks up `win_func_0` in context ✓

But the evaluator falls back to NULL (line 509 in expression.rs) when not found.

### Suspected Issue
Looking at the Window operator's plan node execution (planner.rs lines 348-372), the code extracts WindowFunction from Expression:

```rust
for (name, expr) in window_functions {
    if let Expression::WindowFunction(ref wf) = expr {
        window_funcs.push((name.clone(), wf.clone()));
    }
}
```

**But**: The window_functions are stored as `Vec<(String, Expression)>` where the Expression might not be WindowFunction after planning! They should already be WindowFunction objects.

This mismatch could cause:
- Window function not being executed
- Column not being added to output
- Reference failing in projection

## 6. Testing

### Test File
- **File**: `test_window_function_execution.rs`
- **Test Case**: `e.salary - AVG(e.salary) OVER (PARTITION BY e.department_id)`
- **Expected**: Non-NULL result
- **Actual**: NULL (before fix)

### Existing Tests
- **File**: `src/executor/tests/window_functions.rs`
- **Focus**: Parsing tests, not execution
- **Gap**: No integration tests for window functions in arithmetic

## 7. Recommendations

### Fix Strategy
1. Verify Window node receives correct WindowFunction objects
2. Ensure WindowOperator outputs columns with correct names
3. Add debug logging to trace column name mapping
4. Test with actual execution to verify column values

### Additional Investigation Needed
1. Check if window_functions tuple structure is correct during execution
2. Verify column name propagation from Window to Project
3. Add logging to trace evaluation context creation
4. Test window function result values in isolation

### Code Changes Required
1. Fix Window node execution to handle Expression::WindowFunction correctly
2. Add validation that column names match between Window output and Project reference
3. Add comprehensive logging for debugging
4. Add integration tests for window functions in expressions

## 8. Key Files

1. **Parsing**: `src/sql/ast.rs`, `src/sql/parser.rs`
2. **Planning**: `src/executor/planner.rs`
3. **Execution**: `src/executor/operators.rs` (WindowOperator, ProjectOperator)
4. **Expression Evaluation**: `src/executor/expression.rs`
5. **Tests**: `src/executor/tests/window_functions.rs`, `test_window_function_execution.rs`

## 9. Next Steps

1. Add debug logging to WindowOperator to verify:
   - Input column names
   - Window function names
   - Output column names
2. Add debug logging to ProjectOperator to verify:
   - Input column names from Window
   - Evaluation context column names
   - Column lookup results
3. Run test case with logging to identify exact failure point
4. Fix identified issue
5. Add regression test
