//! Stored procedure execution module
//!
//! Handles execution of stored procedures and functions with support for
//! procedural language constructs, variables, and control flow

use crate::{Result, sql::ast::{*, Statement}};
use crate::executor::{ExecutionContext, ExpressionEvaluator, EvaluationContext, QueryResult};
use crate::types::{Value, ValueKind};
use std::collections::HashMap;

/// Procedure execution state
#[derive(Debug)]
pub struct ProcedureContext {
    /// Local variables in the current scope
    pub variables: HashMap<String, Value>,
    /// Stack of execution frames for nested blocks
    pub call_stack: Vec<ExecutionFrame>,
    /// Output parameters (for OUT and INOUT parameters)
    pub output_params: HashMap<String, Value>,
    /// Return value (for functions)
    pub return_value: Option<Value>,
    /// Exception that is currently being handled
    pub current_exception: Option<ProcedureException>,
    /// Whether we're in a loop that can be exited/exited from
    pub loop_stack: Vec<String>,
}

/// Procedure execution frame representing a block scope
#[derive(Debug)]
pub struct ExecutionFrame {
    /// Local variables in this frame
    pub variables: HashMap<String, Value>,
    /// Cursor definitions (for cursor-based operations)
    pub cursors: HashMap<String, CursorState>,
    /// Label for this block (if any)
    pub label: Option<String>,
}

/// Cursor state for iterating over query results
#[derive(Debug)]
pub struct CursorState {
    /// Query result set
    pub result_set: Option<crate::executor::QueryResult>,
    /// Current position in the result set
    pub current_row: usize,
    /// Whether the cursor is open
    pub is_open: bool,
}

/// Procedure exception information
#[derive(Debug, Clone)]
pub struct ProcedureException {
    pub condition: ExceptionCondition,
    pub message: Option<String>,
    pub sqlstate: String,
}

/// Procedure execution engine
#[derive(Debug)]
pub struct ProcedureExecutor {
    /// Stored procedure definitions
    procedures: HashMap<String, ProcedureDef>,
    /// Stored function definitions
    functions: HashMap<String, FunctionDef>,
}

/// Stored procedure definition
#[derive(Debug, Clone)]
pub struct ProcedureDef {
    pub name: String,
    pub parameters: Vec<ProcedureParameter>,
    pub language: ProcedureLanguage,
    pub body: BlockStatement,
    pub security_definer: bool,
}

/// Stored function definition
#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub parameters: Vec<ProcedureParameter>,
    pub return_type: crate::types::DataType,
    pub language: ProcedureLanguage,
    pub body: BlockStatement,
    pub security_definer: bool,
    pub returns_setof: bool,
}

impl ProcedureExecutor {
    /// Create a new procedure executor
    pub fn new() -> Self {
        Self {
            procedures: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    /// Register a stored procedure
    pub fn register_procedure(&mut self, procedure: CreateProcedureStatement) -> Result<()> {
        let def = ProcedureDef {
            name: procedure.procedure_name.clone(),
            parameters: procedure.parameters,
            language: procedure.language,
            body: procedure.body,
            security_definer: procedure.security_definer,
        };

        self.procedures.insert(procedure.procedure_name, def);
        Ok(())
    }

    /// Register a stored function
    pub fn register_function(&mut self, function: CreateFunctionStatement) -> Result<()> {
        let def = FunctionDef {
            name: function.function_name.clone(),
            parameters: function.parameters,
            return_type: function.return_type,
            language: function.language,
            body: function.body,
            security_definer: function.security_definer,
            returns_setof: function.returns_setof,
        };

        self.functions.insert(function.function_name, def);
        Ok(())
    }

    /// Execute a stored procedure
    pub fn execute_procedure(
        &mut self,
        name: &str,
        arguments: Vec<Value>,
        context: &mut ExecutionContext,
    ) -> Result<QueryResult> {
        let procedure = self.procedures.get(name)
            .ok_or_else(|| crate::error::RustgreSQLError::Procedure(format!("Procedure '{}' does not exist", name)))?
            .clone();

        // Validate argument count
        if arguments.len() != procedure.parameters.len() {
            return Err(crate::error::RustgreSQLError::Procedure(
                format!("Procedure '{}' expects {} arguments, got {}",
                    name, procedure.parameters.len(), arguments.len())
            ));
        }

        // Create procedure execution context
        let mut proc_context = ProcedureContext::new();

        // Initialize parameters
        for (param, arg) in procedure.parameters.iter().zip(arguments.iter()) {
            match param.mode {
                ParameterMode::In | ParameterMode::InOut => {
                    proc_context.variables.insert(param.name.clone(), arg.clone());
                }
                ParameterMode::Out => {
                    // OUT parameters start as NULL
                    proc_context.variables.insert(
                        param.name.clone(),
                        Value::null()
                    );
                }
            }
        }

        // Execute the procedure body
        match self.execute_block(&procedure.body, &mut proc_context, context) {
            Ok(_) => {
                // Collect OUT and INOUT parameters for the result
                let mut result_rows = Vec::new();
                for param in &procedure.parameters {
                    if matches!(param.mode, ParameterMode::Out | ParameterMode::InOut) {
                        if let Some(value) = proc_context.variables.get(&param.name) {
                            result_rows.push(vec![value.clone()]);
                        }
                    }
                }

                Ok(QueryResult {
                    rows: result_rows,
                    column_names: vec!["result".to_string()],
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Execute a stored function
    pub fn execute_function(
        &mut self,
        name: &str,
        arguments: Vec<Value>,
        context: &mut ExecutionContext,
    ) -> Result<Value> {
        let function = self.functions.get(name)
            .ok_or_else(|| crate::error::RustgreSQLError::Procedure(format!("Function '{}' does not exist", name)))?
            .clone();

        // Validate argument count
        if arguments.len() != function.parameters.len() {
            return Err(crate::error::RustgreSQLError::Procedure(
                format!("Function '{}' expects {} arguments, got {}",
                    name, function.parameters.len(), arguments.len())
            ));
        }

        // Create procedure execution context
        let mut proc_context = ProcedureContext::new();

        // Initialize parameters (functions only support IN parameters currently)
        for (param, arg) in function.parameters.iter().zip(arguments.iter()) {
            proc_context.variables.insert(param.name.clone(), arg.clone());
        }

        // Execute the function body
        self.execute_block(&function.body, &mut proc_context, context)?;

        // Return the return value
        proc_context.return_value.ok_or_else(|| {
            crate::error::RustgreSQLError::Procedure(format!("Function '{}' did not return a value", name))
        })
    }

    /// Execute a block statement
    fn execute_block(
        &mut self,
        block: &BlockStatement,
        proc_context: &mut ProcedureContext,
        context: &mut ExecutionContext,
    ) -> Result<()> {
        // Push new execution frame
        let frame = ExecutionFrame {
            variables: HashMap::new(),
            cursors: HashMap::new(),
            label: None, // TODO: Handle block labels
        };
        proc_context.call_stack.push(frame);

        // Process declarations
        for declaration in &block.declarations {
            let value = if let Some(expr) = &declaration.default_value {
                self.evaluate_expression(expr, proc_context, context)?
            } else {
                Value::null()
            };

            proc_context.variables.insert(declaration.name.clone(), value);
        }

        // Execute statements
        for statement in &block.statements {
            // Check if we're returning from this block
            if proc_context.return_value.is_some() {
                break;
            }

            match self.execute_statement(statement, proc_context, context) {
                Ok(_) => continue,
                Err(e) => {
                    // Handle exception if there's an exception handler
                    if let Some(exception_handler) = &block.exception_handler {
                        if self.handle_exception(&e, exception_handler, proc_context, context)? {
                            // Exception was handled, continue with next statement
                            continue;
                        }
                    }
                    // No exception handler or couldn't handle, propagate the error
                    return Err(e);
                }
            }
        }

        // Pop execution frame
        proc_context.call_stack.pop();

        Ok(())
    }

    /// Execute a single statement within a procedure
    fn execute_statement(
        &mut self,
        statement: &Statement,
        proc_context: &mut ProcedureContext,
        context: &mut ExecutionContext,
    ) -> Result<()> {
        match statement {
            // SQL statements that can be executed within procedures
            Statement::Select(select_stmt) => {
                // Execute SELECT but don't return results (unless in cursor context)
                let mut executor = crate::executor::Executor::new();
                let result = executor.execute_statement(&Statement::Select(select_stmt.clone()))?;
                context.log(&format!("SELECT executed within procedure, {} rows affected", result.rows.len()));
                Ok(())
            }

            Statement::Insert(insert_stmt) => {
                let mut executor = crate::executor::Executor::new();
                executor.execute_statement(&Statement::Insert(insert_stmt.clone()))?;
                Ok(())
            }

            Statement::Update(update_stmt) => {
                let mut executor = crate::executor::Executor::new();
                executor.execute_statement(&Statement::Update(update_stmt.clone()))?;
                Ok(())
            }

            Statement::Delete(delete_stmt) => {
                let mut executor = crate::executor::Executor::new();
                executor.execute_statement(&Statement::Delete(delete_stmt.clone()))?;
                Ok(())
            }

            // Control flow statements
            Statement::Return(return_stmt) => {
                let value = if let Some(expr) = &return_stmt.expression {
                    Some(self.evaluate_expression(expr, proc_context, context)?)
                } else {
                    None
                };
                proc_context.return_value = value;
                Ok(())
            }

            Statement::IfStatement(if_stmt) => {
                self.execute_if_statement(if_stmt, proc_context, context)
            }

            Statement::CaseStatement(case_stmt) => {
                self.execute_case_statement(case_stmt, proc_context, context)
            }

            Statement::LoopStatement(loop_stmt) => {
                self.execute_loop_statement(loop_stmt, proc_context, context)
            }

            Statement::WhileStatement(while_stmt) => {
                self.execute_while_statement(while_stmt, proc_context, context)
            }

            Statement::ForStatement(for_stmt) => {
                self.execute_for_statement(for_stmt, proc_context, context)
            }

            Statement::Exit(exit_stmt) => {
                self.handle_exit_statement(exit_stmt, proc_context)
            }

            Statement::Continue(continue_stmt) => {
                self.handle_continue_statement(continue_stmt, proc_context)
            }

            Statement::Declare(declare_stmt) => {
                self.execute_declare_statement(declare_stmt, proc_context, context)
            }

            Statement::RaiseStatement(raise_stmt) => {
                self.execute_raise_statement(raise_stmt, proc_context)
            }

            Statement::CallProcedure(call_stmt) => {
                // Nested procedure call
                let mut args = Vec::new();
                for arg in &call_stmt.arguments {
                    args.push(self.evaluate_expression(arg, proc_context, context)?);
                }

                let _result = self.execute_procedure(&call_stmt.procedure_name, args, context)?;
                Ok(())
            }

            Statement::Perform(perform_stmt) => {
                // Execute expression and discard result
                self.evaluate_expression(&perform_stmt.expression, proc_context, context)?;
                Ok(())
            }

            // Block statements
            Statement::Block(block_stmt) => {
                self.execute_block(block_stmt, proc_context, context)
            }

            // Other statements that shouldn't appear in procedures
            stmt => Err(crate::error::RustgreSQLError::Procedure(
                format!("Statement type '{:?}' not supported in procedures", stmt)
            )),
        }
    }

    /// Execute an IF statement
    fn execute_if_statement(
        &mut self,
        if_stmt: &IfStatement,
        proc_context: &mut ProcedureContext,
        context: &mut ExecutionContext,
    ) -> Result<()> {
        let condition_value = self.evaluate_expression(&if_stmt.condition, proc_context, context)?;

        if condition_value.is_truthy() {
            for stmt in &if_stmt.then_statements {
                self.execute_statement(stmt, proc_context, context)?;

                // Check if we're returning
                if proc_context.return_value.is_some() {
                    break;
                }
            }
        } else {
            // Check ELSIF branches
            for elsif_branch in &if_stmt.elsif_branches {
                let elsif_condition = self.evaluate_expression(&elsif_branch.condition, proc_context, context)?;

                if elsif_condition.is_truthy() {
                    for stmt in &elsif_branch.statements {
                        self.execute_statement(stmt, proc_context, context)?;

                        // Check if we're returning
                        if proc_context.return_value.is_some() {
                            break;
                        }
                    }
                    return Ok(());
                }
            }

            // ELSE branch
            if let Some(else_statements) = &if_stmt.else_statements {
                for stmt in else_statements {
                    self.execute_statement(stmt, proc_context, context)?;

                    // Check if we're returning
                    if proc_context.return_value.is_some() {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute a CASE statement
    fn execute_case_statement(
        &mut self,
        case_stmt: &CaseStatement,
        proc_context: &mut ProcedureContext,
        context: &mut ExecutionContext,
    ) -> Result<()> {
        // Simple CASE (with expression)
        if let Some(case_expr) = &case_stmt.expression {
            let case_value = self.evaluate_expression(case_expr, proc_context, context)?;

            for when_branch in &case_stmt.when_branches {
                let when_value = self.evaluate_expression(&when_branch.condition, proc_context, context)?;

                if case_value.equals(&when_value) {
                    for stmt in &when_branch.statements {
                        self.execute_statement(stmt, proc_context, context)?;

                        // Check if we're returning
                        if proc_context.return_value.is_some() {
                            break;
                        }
                    }
                    return Ok(());
                }
            }
        } else {
            // Searched CASE (boolean conditions)
            for when_branch in &case_stmt.when_branches {
                let condition_value = self.evaluate_expression(&when_branch.condition, proc_context, context)?;

                if condition_value.is_truthy() {
                    for stmt in &when_branch.statements {
                        self.execute_statement(stmt, proc_context, context)?;

                        // Check if we're returning
                        if proc_context.return_value.is_some() {
                            break;
                        }
                    }
                    return Ok(());
                }
            }
        }

        // ELSE branch
        if let Some(else_statements) = &case_stmt.else_statements {
            for stmt in else_statements {
                self.execute_statement(stmt, proc_context, context)?;

                // Check if we're returning
                if proc_context.return_value.is_some() {
                    break;
                }
            }
        }

        Ok(())
    }

    /// Execute a LOOP statement
    fn execute_loop_statement(
        &mut self,
        loop_stmt: &LoopStatement,
        proc_context: &mut ProcedureContext,
        context: &mut ExecutionContext,
    ) -> Result<()> {
        let label = loop_stmt.label.clone();

        // Push loop label onto stack for EXIT/CONTINUE
        if let Some(ref label_name) = label {
            proc_context.loop_stack.push(label_name.clone());
        }

        loop {
            for stmt in &loop_stmt.statements {
                self.execute_statement(stmt, proc_context, context)?;

                // Check if we're returning or exiting the loop
                if proc_context.return_value.is_some() {
                    // Clean up loop stack
                    if label.is_some() {
                        proc_context.loop_stack.pop();
                    }
                    return Ok(());
                }
            }
        }
    }

    /// Execute a WHILE statement
    fn execute_while_statement(
        &mut self,
        while_stmt: &WhileStatement,
        proc_context: &mut ProcedureContext,
        context: &mut ExecutionContext,
    ) -> Result<()> {
        let label = while_stmt.label.clone();

        // Push loop label onto stack for EXIT/CONTINUE
        if let Some(ref label_name) = label {
            proc_context.loop_stack.push(label_name.clone());
        }

        loop {
            let condition_value = self.evaluate_expression(&while_stmt.condition, proc_context, context)?;

            if !condition_value.is_truthy() {
                break;
            }

            for stmt in &while_stmt.statements {
                self.execute_statement(stmt, proc_context, context)?;

                // Check if we're returning or exiting the loop
                if proc_context.return_value.is_some() {
                    // Clean up loop stack
                    if label.is_some() {
                        proc_context.loop_stack.pop();
                    }
                    return Ok(());
                }
            }
        }

        // Clean up loop stack
        if label.is_some() {
            proc_context.loop_stack.pop();
        }

        Ok(())
    }

    /// Execute a FOR statement
    fn execute_for_statement(
        &mut self,
        for_stmt: &ForStatement,
        proc_context: &mut ProcedureContext,
        context: &mut ExecutionContext,
    ) -> Result<()> {
        match for_stmt {
            ForStatement::Integer { variable, lower_bound, upper_bound, statements, label, reverse } => {
                let label_name = label.clone();

                // Push loop label onto stack for EXIT/CONTINUE
                if let Some(ref label) = label_name {
                    proc_context.loop_stack.push(label.clone());
                }

                let lower_val = self.evaluate_expression(lower_bound, proc_context, context)?;
                let upper_val = self.evaluate_expression(upper_bound, proc_context, context)?;

                if let (ValueKind::Integer(lower), ValueKind::Integer(upper)) = (&lower_val.kind, &upper_val.kind) {
                    let range: Vec<i64> = if *reverse {
                        (*upper..=*lower).rev().collect()
                    } else {
                        (*lower..=*upper).collect()
                    };

                    for i in range {
                        // Set loop variable
                        proc_context.variables.insert(variable.clone(), Value::integer(i));

                        // Execute statements
                        for stmt in statements {
                            self.execute_statement(stmt, proc_context, context)?;

                            // Check if we're returning or exiting the loop
                            if proc_context.return_value.is_some() {
                                // Clean up loop stack
                                if label_name.is_some() {
                                    proc_context.loop_stack.pop();
                                }
                                return Ok(());
                            }
                        }
                    }
                } else {
                    return Err(crate::error::RustgreSQLError::Procedure(
                        "FOR loop bounds must be integers".to_string()
                    ));
                }

                // Clean up loop stack
                if label_name.is_some() {
                    proc_context.loop_stack.pop();
                }
            }

            ForStatement::Cursor { cursor_name, query, statements, label } => {
                // TODO: Implement cursor-based FOR loops
                return Err(crate::error::RustgreSQLError::Procedure(
                    "Cursor-based FOR loops not yet implemented".to_string()
                ));
            }
        }

        Ok(())
    }

    /// Handle EXIT statement
    fn handle_exit_statement(
        &mut self,
        exit_stmt: &ExitStatement,
        proc_context: &mut ProcedureContext,
    ) -> Result<()> {
        if let Some(when_condition) = &exit_stmt.when_condition {
            let condition_value = self.evaluate_expression(when_condition, proc_context, &mut ExecutionContext::new())?;
            if !condition_value.is_truthy() {
                return Ok(());
            }
        }

        // TODO: Implement proper loop exit with label matching
        // For now, we'll simulate with a return
        proc_context.return_value = Some(Value::null());

        Ok(())
    }

    /// Handle CONTINUE statement
    fn handle_continue_statement(
        &mut self,
        continue_stmt: &ContinueStatement,
        proc_context: &mut ProcedureContext,
    ) -> Result<()> {
        if let Some(when_condition) = &continue_stmt.when_condition {
            let condition_value = self.evaluate_expression(when_condition, proc_context, &mut ExecutionContext::new())?;
            if !condition_value.is_truthy() {
                return Ok(());
            }
        }

        // TODO: Implement proper continue with label matching
        // For now, we'll just return early to continue to next iteration
        Ok(())
    }

    /// Execute a DECLARE statement
    fn execute_declare_statement(
        &mut self,
        declare_stmt: &DeclareStatement,
        proc_context: &mut ProcedureContext,
        context: &mut ExecutionContext,
    ) -> Result<()> {
        for declaration in &declare_stmt.declarations {
            let value = if let Some(expr) = &declaration.default_value {
                self.evaluate_expression(expr, proc_context, context)?
            } else {
                Value::null()
            };

            proc_context.variables.insert(declaration.name.clone(), value);
        }

        Ok(())
    }

    /// Execute a RAISE statement
    fn execute_raise_statement(
        &mut self,
        raise_stmt: &RaiseStatement,
        proc_context: &mut ProcedureContext,
    ) -> Result<()> {
        let condition = if let Some(condition_name) = &raise_stmt.condition {
            ExceptionCondition::Specific(condition_name.clone())
        } else {
            ExceptionCondition::Others
        };

        let exception = ProcedureException {
            condition,
            message: raise_stmt.message.clone(),
            sqlstate: match raise_stmt.level {
                RaiseLevel::Exception => "P0001".to_string(),
                _ => "01000".to_string(),
            },
        };

        proc_context.current_exception = Some(exception.clone());

        Err(crate::error::RustgreSQLError::Procedure(format!(
            "RAISE {}: {}",
            match raise_stmt.level {
                RaiseLevel::Debug => "DEBUG",
                RaiseLevel::Log => "LOG",
                RaiseLevel::Info => "INFO",
                RaiseLevel::Notice => "NOTICE",
                RaiseLevel::Warning => "WARNING",
                RaiseLevel::Exception => "EXCEPTION",
            },
            raise_stmt.message.as_deref().unwrap_or("No message")
        )))
    }

    /// Handle exceptions using the provided exception handler
    fn handle_exception(
        &mut self,
        error: &crate::error::RustgreSQLError,
        exception_handler: &ExceptionHandler,
        proc_context: &mut ProcedureContext,
        context: &mut ExecutionContext,
    ) -> Result<bool> {
        // TODO: Implement proper exception handling with condition matching
        // For now, execute OTHERS handler if available

        for condition in &exception_handler.conditions {
            match condition {
                ExceptionCondition::Others => {
                    // Execute handler statements
                    for stmt in &exception_handler.statements {
                        self.execute_statement(stmt, proc_context, context)?;
                    }
                    return Ok(true);
                }
                _ => {
                    // TODO: Implement specific exception condition matching
                }
            }
        }

        Ok(false)
    }

    /// Evaluate an expression in the procedure context
    fn evaluate_expression(
        &mut self,
        expr: &Expression,
        proc_context: &ProcedureContext,
        context: &mut ExecutionContext,
    ) -> Result<Value> {
        match expr {
            Expression::Column { table: _, name } => {
                // Look up variable in procedure context
                proc_context.variables.get(name)
                    .cloned()
                    .ok_or_else(|| crate::error::RustgreSQLError::Procedure(format!("Variable '{}' not found", name)))
            }

            Expression::Value(value) => Ok(value.clone()),
            Expression::Literal(value) => Ok(value.clone()),

            Expression::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_expression(left, proc_context, context)?;
                let right_val = self.evaluate_expression(right, proc_context, context)?;
                self.evaluate_binary_op(*op, &left_val, &right_val)
            }

            Expression::UnaryOp { op, expr } => {
                let val = self.evaluate_expression(expr, proc_context, context)?;
                self.evaluate_unary_op(*op, &val)
            }

            Expression::Function { name, args } => {
                let mut arg_values = Vec::new();
                for arg in args {
                    arg_values.push(self.evaluate_expression(arg, proc_context, context)?);
                }

                // Check if it's a built-in function first
                if let Ok(result) = self.evaluate_builtin_function(name, &arg_values) {
                    return Ok(result);
                }

                // Otherwise, try to execute as a user-defined function
                self.execute_function(name, arg_values, context)
            }

            Expression::Parameter(idx) => {
                // Parameters should have been resolved to variables by now
                Err(crate::error::RustgreSQLError::Procedure(
                    format!("Unresolved parameter ${} in procedure", idx)
                ))
            }

            _ => {
                // For other expression types, we'll use the main expression evaluator
                // with a custom evaluation context that includes procedure variables
                let mut eval_context = EvaluationContext::new();

                // Add procedure variables to the evaluation context
                for (name, value) in &proc_context.variables {
                    eval_context.set_variable(name, value.clone());
                }

                let evaluator = ExpressionEvaluator::new();
                evaluator.evaluate(expr, &eval_context)
            }
        }
    }

    /// Evaluate a binary operation
    fn evaluate_binary_op(&self, op: BinaryOperator, left: &Value, right: &Value) -> Result<Value> {
        match op {
            BinaryOperator::Add => left.add(right),
            BinaryOperator::Subtract => left.subtract(right),
            BinaryOperator::Multiply => left.multiply(right),
            BinaryOperator::Divide => left.divide(right),
            BinaryOperator::Equals => Ok(Value::boolean(left.equals(right))),
            BinaryOperator::NotEquals => Ok(Value::boolean(!left.equals(right))),
            BinaryOperator::LessThan => Ok(Value::boolean(left.less_than(right))),
            BinaryOperator::LessThanOrEquals => Ok(Value::boolean(left.less_than_or_equal(right))),
            BinaryOperator::GreaterThan => Ok(Value::boolean(left.greater_than(right))),
            BinaryOperator::GreaterThanOrEquals => Ok(Value::boolean(left.greater_than_or_equal(right))),
            BinaryOperator::And => Ok(Value::boolean(left.is_truthy() && right.is_truthy())),
            BinaryOperator::Or => Ok(Value::boolean(left.is_truthy() || right.is_truthy())),
            _ => Err(crate::error::RustgreSQLError::Procedure(
                format!("Binary operator {:?} not yet implemented in procedures", op)
            )),
        }
    }

    /// Evaluate a unary operation
    fn evaluate_unary_op(&self, op: UnaryOperator, operand: &Value) -> Result<Value> {
        match op {
            UnaryOperator::Not => Ok(Value::boolean(!operand.is_truthy())),
            UnaryOperator::Minus => operand.negate(),
            UnaryOperator::Plus => Ok(operand.clone()),
        }
    }

    /// Evaluate built-in functions
    fn evaluate_builtin_function(&self, name: &str, args: &[Value]) -> Result<Value> {
        match name.to_uppercase().as_str() {
            "ABS" => {
                if args.len() != 1 {
                    return Err(crate::error::RustgreSQLError::Procedure("ABS() requires exactly 1 argument".to_string()));
                }
                args[0].abs()
            }

            "COALESCE" => {
                if args.is_empty() {
                    return Err(crate::error::RustgreSQLError::Procedure("COALESCE() requires at least 1 argument".to_string()));
                }

                for arg in args {
                    if !arg.is_null() {
                        return Ok(arg.clone());
                    }
                }
                Ok(args.last().unwrap().clone())
            }

            "LENGTH" | "LEN" => {
                if args.len() != 1 {
                    return Err(crate::error::RustgreSQLError::Procedure("LENGTH() requires exactly 1 argument".to_string()));
                }

                match &args[0].kind {
                    crate::types::ValueKind::String(s) => Ok(Value::integer(s.len() as i64)),
                    _ => Err(crate::error::RustgreSQLError::Procedure("LENGTH() argument must be a string".to_string())),
                }
            }

            "UPPER" => {
                if args.len() != 1 {
                    return Err(crate::error::RustgreSQLError::Procedure("UPPER() requires exactly 1 argument".to_string()));
                }

                match &args[0].kind {
                    crate::types::ValueKind::String(s) => Ok(Value::string(s.to_uppercase())),
                    _ => Err(crate::error::RustgreSQLError::Procedure("UPPER() argument must be a string".to_string())),
                }
            }

            "LOWER" => {
                if args.len() != 1 {
                    return Err(crate::error::RustgreSQLError::Procedure("LOWER() requires exactly 1 argument".to_string()));
                }

                match &args[0].kind {
                    crate::types::ValueKind::String(s) => Ok(Value::string(s.to_lowercase())),
                    _ => Err(crate::error::RustgreSQLError::Procedure("LOWER() argument must be a string".to_string())),
                }
            }

            _ => Err(crate::error::RustgreSQLError::Procedure(
                format!("Unknown built-in function: {}", name)
            )),
        }
    }
}

impl ProcedureContext {
    /// Create a new procedure execution context
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            call_stack: Vec::new(),
            output_params: HashMap::new(),
            return_value: None,
            current_exception: None,
            loop_stack: Vec::new(),
        }
    }
}

impl ExecutionFrame {
    /// Create a new execution frame
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            cursors: HashMap::new(),
            label: None,
        }
    }

    /// Create a new execution frame with a label
    pub fn with_label(label: String) -> Self {
        Self {
            variables: HashMap::new(),
            cursors: HashMap::new(),
            label: Some(label),
        }
    }
}

impl CursorState {
    /// Create a new cursor state
    pub fn new() -> Self {
        Self {
            result_set: None,
            current_row: 0,
            is_open: false,
        }
    }
}

// Extension trait for Value to add truthiness
trait ValueExt {
    fn is_truthy(&self) -> bool;
    fn is_null(&self) -> bool;
    fn equals(&self, other: &Value) -> bool;
    fn less_than(&self, other: &Value) -> bool;
    fn less_than_or_equal(&self, other: &Value) -> bool;
    fn greater_than(&self, other: &Value) -> bool;
    fn greater_than_or_equal(&self, other: &Value) -> bool;
    fn add(&self, other: &Value) -> Result<Value>;
    fn subtract(&self, other: &Value) -> Result<Value>;
    fn multiply(&self, other: &Value) -> Result<Value>;
    fn divide(&self, other: &Value) -> Result<Value>;
    fn negate(&self) -> Result<Value>;
    fn abs(&self) -> Result<Value>;
}

impl ValueExt for Value {
    fn is_truthy(&self) -> bool {
        match &self.kind {
            crate::types::ValueKind::Null(_) => false,
            crate::types::ValueKind::Boolean(b) => *b,
            crate::types::ValueKind::Integer(i) => *i != 0,
            crate::types::ValueKind::Float(f) => *f != 0.0,
            crate::types::ValueKind::String(s) => !s.is_empty(),
            crate::types::ValueKind::Timestamp(_) => true,
        }
    }

    fn is_null(&self) -> bool {
        matches!(&self.kind, crate::types::ValueKind::Null(_))
    }

    fn equals(&self, other: &Value) -> bool {
        match (&self.kind, &other.kind) {
            (crate::types::ValueKind::Null(_), crate::types::ValueKind::Null(_)) => true,
            (crate::types::ValueKind::Null(_), _) | (_, crate::types::ValueKind::Null(_)) => false,
            (crate::types::ValueKind::Boolean(a), crate::types::ValueKind::Boolean(b)) => a == b,
            (crate::types::ValueKind::Integer(a), crate::types::ValueKind::Integer(b)) => a == b,
            (crate::types::ValueKind::Float(a), crate::types::ValueKind::Float(b)) => a == b,
            (crate::types::ValueKind::Integer(a), crate::types::ValueKind::Float(b)) => (*a as f64) == *b,
            (crate::types::ValueKind::Float(a), crate::types::ValueKind::Integer(b)) => *a == (*b as f64),
            (crate::types::ValueKind::String(a), crate::types::ValueKind::String(b)) => a == b,
            _ => false,
        }
    }

    fn less_than(&self, other: &Value) -> bool {
        match (&self.kind, &other.kind) {
            (crate::types::ValueKind::Integer(a), crate::types::ValueKind::Integer(b)) => a < b,
            (crate::types::ValueKind::Float(a), crate::types::ValueKind::Float(b)) => a < b,
            (crate::types::ValueKind::Integer(a), crate::types::ValueKind::Float(b)) => (*a as f64) < *b,
            (crate::types::ValueKind::Float(a), crate::types::ValueKind::Integer(b)) => *a < (*b as f64),
            (crate::types::ValueKind::String(a), crate::types::ValueKind::String(b)) => a < b,
            _ => false,
        }
    }

    fn less_than_or_equal(&self, other: &Value) -> bool {
        self.less_than(other) || self.equals(other)
    }

    fn greater_than(&self, other: &Value) -> bool {
        !self.less_than_or_equal(other)
    }

    fn greater_than_or_equal(&self, other: &Value) -> bool {
        !self.less_than(other)
    }

    fn add(&self, other: &Value) -> Result<Value> {
        match (&self.kind, &other.kind) {
            (crate::types::ValueKind::Integer(a), crate::types::ValueKind::Integer(b)) => {
                Ok(Value::integer(a + b))
            }
            (crate::types::ValueKind::Float(a), crate::types::ValueKind::Float(b)) => {
                Ok(Value::float(a + b))
            }
            (crate::types::ValueKind::Integer(a), crate::types::ValueKind::Float(b)) => {
                Ok(Value::float((*a as f64) + b))
            }
            (crate::types::ValueKind::Float(a), crate::types::ValueKind::Integer(b)) => {
                Ok(Value::float(a + (*b as f64)))
            }
            (crate::types::ValueKind::String(a), crate::types::ValueKind::String(b)) => {
                Ok(Value::string(a.clone() + b))
            }
            _ => Err(crate::error::RustgreSQLError::Procedure("Invalid types for addition".to_string())),
        }
    }

    fn subtract(&self, other: &Value) -> Result<Value> {
        match (&self.kind, &other.kind) {
            (crate::types::ValueKind::Integer(a), crate::types::ValueKind::Integer(b)) => {
                Ok(Value::integer(a - b))
            }
            (crate::types::ValueKind::Float(a), crate::types::ValueKind::Float(b)) => {
                Ok(Value::float(a - b))
            }
            (crate::types::ValueKind::Integer(a), crate::types::ValueKind::Float(b)) => {
                Ok(Value::float((*a as f64) - b))
            }
            (crate::types::ValueKind::Float(a), crate::types::ValueKind::Integer(b)) => {
                Ok(Value::float(a - (*b as f64)))
            }
            _ => Err(crate::error::RustgreSQLError::Procedure("Invalid types for subtraction".to_string())),
        }
    }

    fn multiply(&self, other: &Value) -> Result<Value> {
        match (&self.kind, &other.kind) {
            (crate::types::ValueKind::Integer(a), crate::types::ValueKind::Integer(b)) => {
                Ok(Value::integer(a * b))
            }
            (crate::types::ValueKind::Float(a), crate::types::ValueKind::Float(b)) => {
                Ok(Value::float(a * b))
            }
            (crate::types::ValueKind::Integer(a), crate::types::ValueKind::Float(b)) => {
                Ok(Value::float((*a as f64) * b))
            }
            (crate::types::ValueKind::Float(a), crate::types::ValueKind::Integer(b)) => {
                Ok(Value::float(a * (*b as f64)))
            }
            _ => Err(crate::error::RustgreSQLError::Procedure("Invalid types for multiplication".to_string())),
        }
    }

    fn divide(&self, other: &Value) -> Result<Value> {
        match (&self.kind, &other.kind) {
            (crate::types::ValueKind::Integer(a), crate::types::ValueKind::Integer(b)) => {
                if *b == 0 {
                    return Err(crate::error::RustgreSQLError::Procedure("Division by zero".to_string()));
                }
                Ok(Value::integer(a / b))
            }
            (crate::types::ValueKind::Float(a), crate::types::ValueKind::Float(b)) => {
                if *b == 0.0 {
                    return Err(crate::error::RustgreSQLError::Procedure("Division by zero".to_string()));
                }
                Ok(Value::float(a / b))
            }
            (crate::types::ValueKind::Integer(a), crate::types::ValueKind::Float(b)) => {
                if *b == 0.0 {
                    return Err(crate::error::RustgreSQLError::Procedure("Division by zero".to_string()));
                }
                Ok(Value::float((*a as f64) / b))
            }
            (crate::types::ValueKind::Float(a), crate::types::ValueKind::Integer(b)) => {
                if *b == 0 {
                    return Err(crate::error::RustgreSQLError::Procedure("Division by zero".to_string()));
                }
                Ok(Value::float(a / (*b as f64)))
            }
            _ => Err(crate::error::RustgreSQLError::Procedure("Invalid types for division".to_string())),
        }
    }

    fn negate(&self) -> Result<Value> {
        match &self.kind {
            crate::types::ValueKind::Integer(a) => Ok(Value::integer(-a)),
            crate::types::ValueKind::Float(a) => Ok(Value::float(-a)),
            _ => Err(crate::error::RustgreSQLError::Procedure("Cannot negate non-numeric value".to_string())),
        }
    }

    fn abs(&self) -> Result<Value> {
        match &self.kind {
            crate::types::ValueKind::Integer(a) => Ok(Value::integer(a.abs())),
            crate::types::ValueKind::Float(a) => Ok(Value::float(a.abs())),
            _ => Err(crate::error::RustgreSQLError::Procedure("ABS() argument must be numeric".to_string())),
        }
    }
}

// Include separate test module
#[cfg(test)]
include!("procedure_tests.rs");
