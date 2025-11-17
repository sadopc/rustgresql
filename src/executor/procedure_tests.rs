// Basic integration tests for stored procedures
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DataType;

    #[test]
    fn test_procedure_executor_creation() {
        let executor = ProcedureExecutor::new();
        assert!(executor.list_procedures().is_empty());
        assert!(executor.list_functions().is_empty());
    }

    #[test]
    fn test_execution_frame_basics() {
        let mut frame = ExecutionFrame::new();

        // Test variable operations
        frame.declare_variable("test_var", DataType::Integer, None);
        assert!(frame.has_variable("test_var"));

        // Test scoping
        frame.enter_scope();
        frame.declare_variable("scoped_var", DataType::Text, None);
        assert!(frame.has_variable("scoped_var"));
        frame.exit_scope();
        assert!(!frame.has_variable("scoped_var"));
        assert!(frame.has_variable("test_var")); // Original variable still exists
    }

    #[test]
    fn test_value_arithmetic() {
        let a = crate::types::Value::integer(10);
        let b = crate::types::Value::integer(5);

        // Test basic arithmetic operations
        let sum = a.add(&b).unwrap();
        assert!(sum.is_integer() && sum.as_integer().unwrap() == 15);

        let diff = a.subtract(&b).unwrap();
        assert!(diff.is_integer() && diff.as_integer().unwrap() == 5);

        let product = a.multiply(&b).unwrap();
        assert!(product.is_integer() && product.as_integer().unwrap() == 50);

        let quotient = a.divide(&b).unwrap();
        assert!(quotient.is_integer() && quotient.as_integer().unwrap() == 2);
    }

    #[test]
    fn test_division_by_zero_error() {
        let a = crate::types::Value::integer(10);
        let zero = crate::types::Value::integer(0);

        let result = a.divide(&zero);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), crate::error::RustgreSQLError::Procedure(_)));
    }

    #[test]
    fn test_procedure_executor_registration() {
        let mut executor = ProcedureExecutor::new();

        // Test function existence checking
        assert!(!executor.function_exists("non_existent"));
        assert!(!executor.procedure_exists("non_existent"));

        // Register simple function and procedure definitions
        let function = FunctionDef {
            name: "test_function".to_string(),
            parameters: vec![],
            return_type: DataType::Integer,
            body: vec![],
            language: "plpgsql".to_string(),
            security_mode: SecurityMode::Invoker,
        };

        executor.register_function(function);
        assert!(executor.function_exists("test_function"));
        assert_eq!(executor.list_functions().len(), 1);

        let procedure = ProcedureDef {
            name: "test_procedure".to_string(),
            parameters: vec![],
            return_type: None,
            body: vec![],
            language: "plpgsql".to_string(),
            security_mode: SecurityMode::Invoker,
        };

        executor.register_procedure(procedure);
        assert!(executor.procedure_exists("test_procedure"));
        assert_eq!(executor.list_procedures().len(), 1);
    }
}