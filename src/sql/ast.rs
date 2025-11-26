//! SQL Abstract Syntax Tree

use serde::{Deserialize, Serialize};

/// SQL statement
#[derive(Debug, Clone)]
pub enum Statement {
    Select(SelectStatement),
    Insert(InsertStatement),
    Update(UpdateStatement),
    Delete(DeleteStatement),
    CreateTable(CreateTableStatement),
    CreateIndex(CreateIndexStatement),
    DropTable(DropTableStatement),
    DropIndex(DropIndexStatement),
    AlterTable(AlterTableStatement),
    CreateView(CreateViewStatement),
    DropView(DropViewStatement),
    RefreshMaterializedView(RefreshMaterializedViewStatement),
    CreateProcedure(CreateProcedureStatement),
    CreateFunction(CreateFunctionStatement),
    DropProcedure(DropProcedureStatement),
    DropFunction(DropFunctionStatement),
    CallProcedure(CallProcedureStatement),
    Perform(PerformStatement),
    // Transaction control
    BeginTransaction,
    CommitTransaction,
    RollbackTransaction,
    // Control flow statements (used within procedures)
    Block(BlockStatement),
    Return(ReturnStatement),
    IfStatement(IfStatement),
    CaseStatement(CaseStatement),
    LoopStatement(LoopStatement),
    WhileStatement(WhileStatement),
    ForStatement(ForStatement),
    Exit(ExitStatement),
    Continue(ContinueStatement),
    Declare(DeclareStatement),
    RaiseStatement(RaiseStatement),
}

/// Column definition
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: crate::types::DataType,
    pub constraints: Vec<ColumnConstraint>,
}

/// Column specification for SELECT (expression with optional alias)
#[derive(Debug, Clone)]
pub struct ColumnSpec {
    pub expr: Expression,
    pub alias: Option<String>,
}

/// Table reference
#[derive(Debug, Clone)]
pub enum TableRef {
    /// Simple table reference with optional alias
    Table {
        name: String,
        alias: Option<String>,
    },
    /// Subquery with optional alias
    Subquery {
        subquery: Box<Statement>,
        alias: Option<String>,
    },
}

/// Window frame clause
#[derive(Debug, Clone)]
pub struct WindowFrame {
    pub mode: WindowFrameMode,
    pub start: WindowFrameBound,
    pub end: Option<WindowFrameBound>,
}

/// Window frame mode (ROWS vs RANGE)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowFrameMode {
    Rows,
    Range,
}

/// Window frame bound
#[derive(Debug, Clone)]
pub enum WindowFrameBound {
    CurrentRow,
    UnboundedPreceding,
    UnboundedFollowing,
    Preceding(Box<Expression>),
    Following(Box<Expression>),
}

/// Window clause for OVER ()
#[derive(Debug, Clone)]
pub struct WindowClause {
    pub partition_by: Vec<Expression>,
    pub order_by: Vec<OrderBy>,
    pub window_frame: Option<WindowFrame>,
}

/// Window function call
#[derive(Debug, Clone)]
pub struct WindowFunction {
    pub name: String,
    pub args: Vec<Expression>,
    pub window_clause: WindowClause,
    pub window_name: Option<String>, // For named window definitions
}

/// Expression in SQL
#[derive(Debug, Clone)]
pub enum Expression {
    /// Column reference
    Column {
        table: Option<String>,
        name: String,
    },
    /// Literal value
    Value(crate::types::Value),
    /// Literal expression (for optimizer compatibility)
    Literal(crate::types::Value),
    /// Function call
    Function {
        name: String,
        args: Vec<Expression>,
        distinct: bool,
    },
    /// Window function call
    WindowFunction(WindowFunction),
    /// Binary operation
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOperator,
        right: Box<Expression>,
    },
    /// Unary operation
    UnaryOp {
        op: UnaryOperator,
        expr: Box<Expression>,
    },
    /// CAST expression
    Cast {
        expr: Box<Expression>,
        data_type: crate::types::DataType,
    },
    /// Subquery
    Subquery(Box<Statement>),
    /// EXISTS subquery
    Exists {
        subquery: Box<Statement>,
        negated: bool,
    },
    /// List of expressions
    List(Vec<Expression>),
    /// Star (SELECT *)
    Star,
    /// Parameter placeholder
    Parameter(usize),
    /// CASE expression
    Case {
        /// Optional base expression for simple CASE (CASE expr WHEN value THEN result)
        base: Option<Box<Expression>>,
        /// WHEN...THEN branches
        branches: Vec<CaseBranch>,
        /// Optional ELSE result
        else_result: Option<Box<Expression>>,
    },
}

/// Branch in a CASE expression
#[derive(Debug, Clone)]
pub struct CaseBranch {
    /// Condition expression (for searched CASE) or value to compare (for simple CASE)
    pub condition: Box<Expression>,
    /// Result expression when condition matches
    pub result: Box<Expression>,
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BinaryOperator {
    Equals,
    NotEquals,
    LessThan,
    LessThanOrEquals,
    GreaterThan,
    GreaterThanOrEquals,
    Like,
    ILike,
    In,
    And,
    Or,
    Is,
    IsNot,
    Add,
    Subtract,
    Multiply,
    Divide,
    Concatenate,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum UnaryOperator {
    Not,
    Minus,
    Plus,
    Exists,
    NotExists,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SortDirection {
    Asc,
    Desc,
}

/// Nulls handling specification
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NullsPosition {
    First,   // NULLS FIRST
    Last,    // NULLS LAST
    Default, // No specification (use database default)
}

/// Order by clause
#[derive(Debug, Clone)]
pub struct OrderBy {
    pub expr: Expression,
    pub direction: SortDirection,
    pub nulls: NullsPosition,
}

/// Join type
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    Cross,         // CROSS JOIN (Cartesian product)
    LeftAnti,      // LEFT ANTI JOIN (NOT EXISTS/NOT IN)
    LeftSemi,      // LEFT SEMI JOIN (EXISTS/IN)
    RightAnti,     // RIGHT ANTI JOIN
    RightSemi,     // RIGHT SEMI JOIN
}

/// Join condition
#[derive(Debug, Clone)]
pub struct JoinCondition {
    pub table: TableRef,
    pub join_type: JoinType,
    pub condition: Option<Expression>,
}

/// Set operation operator
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SetOperator {
    Union,
    Intersect,
    Except,
}

/// Set operation combining two SELECT statements
#[derive(Debug, Clone)]
pub struct SetOperation {
    pub operator: SetOperator,
    pub left: Box<SelectStatement>,
    pub right: Box<SelectStatement>,
    pub all: bool, // for UNION ALL, INTERSECT ALL, EXCEPT ALL
}

/// Named window definition for WINDOW clause
#[derive(Debug, Clone)]
pub struct NamedWindow {
    pub name: String,
    pub window_clause: WindowClause,
}

/// Common Table Expression (CTE) definition
#[derive(Debug, Clone)]
pub struct CommonTableExpression {
    pub name: String,
    pub column_names: Option<Vec<String>>, // Optional column aliases
    pub query: Box<SelectStatement>,
    pub recursive: bool, // For recursive CTEs
}

/// WITH clause containing one or more CTEs
#[derive(Debug, Clone)]
pub struct WithClause {
    pub ctes: Vec<CommonTableExpression>,
    pub recursive: bool, // True if any CTE is recursive
}

/// SELECT statement
#[derive(Debug, Clone)]
pub enum SelectStatement {
    Simple {
        with_clause: Option<WithClause>,
        distinct: bool,
        columns: Vec<ColumnSpec>,
        from: Vec<TableRef>,
        joins: Vec<JoinCondition>,
        where_clause: Option<Expression>,
        group_by: Vec<Expression>,
        having: Option<Expression>,
        order_by: Vec<OrderBy>,
        limit: Option<i64>,
        offset: Option<i64>,
        named_windows: Vec<NamedWindow>, // For WINDOW clause
    },
    SetOperation(SetOperation),
}

/// INSERT statement
#[derive(Debug, Clone)]
pub struct InsertStatement {
    pub table: TableRef,
    pub columns: Vec<String>,
    pub values: Vec<Vec<Expression>>,
}

/// UPDATE statement
#[derive(Debug, Clone)]
pub struct UpdateStatement {
    pub table: TableRef,
    pub assignments: Vec<(String, Expression)>,
    pub where_clause: Option<Expression>,
}

/// DELETE statement
#[derive(Debug, Clone)]
pub struct DeleteStatement {
    pub table: TableRef,
    pub where_clause: Option<Expression>,
}

/// CREATE TABLE statement
#[derive(Debug, Clone)]
pub struct CreateTableStatement {
    pub table_name: String,
    pub columns: Vec<ColumnDef>,
    pub table_constraints: Vec<TableConstraint>,
    pub if_not_exists: bool,
}

/// CREATE INDEX statement
#[derive(Debug, Clone)]
pub struct CreateIndexStatement {
    pub index_name: String,
    pub table_name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub if_not_exists: bool,
}

/// DROP TABLE statement
#[derive(Debug, Clone)]
pub struct DropTableStatement {
    pub table_name: String,
    pub if_exists: bool,
}

/// DROP INDEX statement
#[derive(Debug, Clone)]
pub struct DropIndexStatement {
    pub index_name: String,
    pub if_exists: bool,
}

/// ALTER TABLE operation types
#[derive(Debug, Clone)]
pub enum AlterOperation {
    /// Add a column
    AddColumn {
        column: ColumnDef,
    },
    /// Drop a column
    DropColumn {
        column_name: String,
    },
    /// Rename a column
    RenameColumn {
        old_name: String,
        new_name: String,
    },
    /// Add a constraint
    AddConstraint {
        constraint: TableConstraint,
    },
    /// Drop a constraint
    DropConstraint {
        constraint_name: String,
    },
    /// Rename the table
    RenameTable {
        new_name: String,
    },
}

/// ALTER TABLE statement
#[derive(Debug, Clone)]
pub struct AlterTableStatement {
    pub table_name: String,
    pub operation: AlterOperation,
}

/// CREATE VIEW and CREATE MATERIALIZED VIEW statement
#[derive(Debug, Clone)]
pub struct CreateViewStatement {
    pub view_name: String,
    pub columns: Vec<String>, // Optional column aliases
    pub query: SelectStatement,
    pub materialized: bool,
    pub with_data: bool, // For materialized views
}

/// DROP VIEW and DROP MATERIALIZED VIEW statement
#[derive(Debug, Clone)]
pub struct DropViewStatement {
    pub view_name: String,
    pub materialized: bool,
    pub cascade: bool,
}

/// REFRESH MATERIALIZED VIEW statement
#[derive(Debug, Clone)]
pub struct RefreshMaterializedViewStatement {
    pub view_name: String,
    pub concurrently: bool, // PostgreSQL-style concurrent refresh
    pub with_data: bool,
}

/// Table constraint types
#[derive(Debug, Clone)]
pub enum TableConstraint {
    /// PRIMARY KEY (column1, column2, ...)
    PrimaryKey {
        columns: Vec<String>,
        name: Option<String>,
    },
    /// FOREIGN KEY (column1, column2, ...) REFERENCES table (column1, column2, ...)
    ForeignKey {
        columns: Vec<String>,
        ref_table: String,
        ref_columns: Vec<String>,
        name: Option<String>,
    },
    /// UNIQUE (column1, column2, ...)
    Unique {
        columns: Vec<String>,
        name: Option<String>,
    },
    /// CHECK (condition)
    Check {
        condition: Expression,
        name: Option<String>,
    },
}

/// Column constraint types
#[derive(Debug, Clone)]
pub enum ColumnConstraint {
    /// NOT NULL
    NotNull,
    /// NULL
    Null,
    /// DEFAULT value
    Default(String),
    /// PRIMARY KEY
    PrimaryKey,
    /// UNIQUE
    Unique,
    /// CHECK (condition)
    Check(Expression),
    /// REFERENCES table (column)
    References {
        table: String,
        column: Option<String>,
    },
}

// ===== STORED PROCEDURE AST STRUCTURES =====

/// Procedure parameter
#[derive(Debug, Clone)]
pub struct ProcedureParameter {
    pub name: String,
    pub data_type: crate::types::DataType,
    pub mode: ParameterMode,
    pub default_value: Option<Expression>,
}

/// Parameter mode (IN, OUT, INOUT)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterMode {
    In,
    Out,
    InOut,
}

/// Procedure language
#[derive(Debug, Clone, PartialEq)]
pub enum ProcedureLanguage {
    SQL,
    PLpgSQL,
}

/// CREATE PROCEDURE statement
#[derive(Debug, Clone)]
pub struct CreateProcedureStatement {
    pub procedure_name: String,
    pub parameters: Vec<ProcedureParameter>,
    pub language: ProcedureLanguage,
    pub body: BlockStatement,
    pub or_replace: bool,
    pub security_definer: bool, // SECURITY DEFINER vs INVOKER
}

/// CREATE FUNCTION statement
#[derive(Debug, Clone)]
pub struct CreateFunctionStatement {
    pub function_name: String,
    pub parameters: Vec<ProcedureParameter>,
    pub return_type: crate::types::DataType,
    pub language: ProcedureLanguage,
    pub body: BlockStatement,
    pub or_replace: bool,
    pub security_definer: bool,
    pub returns_setof: bool, // RETURNS SETOF
}

/// DROP PROCEDURE statement
#[derive(Debug, Clone)]
pub struct DropProcedureStatement {
    pub procedure_name: String,
    pub if_exists: bool,
    pub parameters: Vec<crate::types::DataType>, // For overloaded procedures
}

/// DROP FUNCTION statement
#[derive(Debug, Clone)]
pub struct DropFunctionStatement {
    pub function_name: String,
    pub if_exists: bool,
    pub parameters: Vec<crate::types::DataType>, // For overloaded functions
    pub cascade: bool,
}

/// CALL procedure statement
#[derive(Debug, Clone)]
pub struct CallProcedureStatement {
    pub procedure_name: String,
    pub arguments: Vec<Expression>,
}

/// PERFORM statement (executes a procedure and discards results)
#[derive(Debug, Clone)]
pub struct PerformStatement {
    pub expression: Expression,
}

/// Block statement (BEGIN...END)
#[derive(Debug, Clone)]
pub struct BlockStatement {
    pub declarations: Vec<Declaration>,
    pub statements: Vec<Statement>,
    pub exception_handler: Option<ExceptionHandler>,
}

/// Variable declaration
#[derive(Debug, Clone)]
pub struct Declaration {
    pub name: String,
    pub data_type: crate::types::DataType,
    pub default_value: Option<Expression>,
    pub constant: bool, // CONSTANT keyword
}

/// Exception handler
#[derive(Debug, Clone)]
pub struct ExceptionHandler {
    pub conditions: Vec<ExceptionCondition>,
    pub statements: Vec<Statement>,
}

/// Exception condition
#[derive(Debug, Clone)]
pub enum ExceptionCondition {
    Specific(String), // Specific exception name
    When(Vec<String>), // WHEN condition1 OR condition2
    Others, // WHEN OTHERS
}

/// RETURN statement
#[derive(Debug, Clone)]
pub struct ReturnStatement {
    pub expression: Option<Expression>, // None for procedures with no return value
}

/// IF statement
#[derive(Debug, Clone)]
pub struct IfStatement {
    pub condition: Expression,
    pub then_statements: Vec<Statement>,
    pub elsif_branches: Vec<ElsifBranch>,
    pub else_statements: Option<Vec<Statement>>,
}

/// ELSIF branch
#[derive(Debug, Clone)]
pub struct ElsifBranch {
    pub condition: Expression,
    pub statements: Vec<Statement>,
}

/// CASE statement
#[derive(Debug, Clone)]
pub struct CaseStatement {
    pub expression: Option<Expression>, // None for simple CASE
    pub when_branches: Vec<WhenBranch>,
    pub else_statements: Option<Vec<Statement>>,
}

/// WHEN branch
#[derive(Debug, Clone)]
pub struct WhenBranch {
    pub condition: Expression,
    pub statements: Vec<Statement>,
}

/// LOOP statement (infinite loop)
#[derive(Debug, Clone)]
pub struct LoopStatement {
    pub statements: Vec<Statement>,
    pub label: Option<String>,
}

/// WHILE statement
#[derive(Debug, Clone)]
pub struct WhileStatement {
    pub condition: Expression,
    pub statements: Vec<Statement>,
    pub label: Option<String>,
}

/// FOR statement (cursor or integer)
#[derive(Debug, Clone)]
pub enum ForStatement {
    Cursor {
        cursor_name: String,
        query: Box<SelectStatement>,
        statements: Vec<Statement>,
        label: Option<String>,
    },
    Integer {
        variable: String,
        lower_bound: Expression,
        upper_bound: Expression,
        statements: Vec<Statement>,
        label: Option<String>,
        reverse: bool, // REVERSE keyword
    },
}

/// EXIT statement
#[derive(Debug, Clone)]
pub struct ExitStatement {
    pub label: Option<String>,
    pub when_condition: Option<Expression>,
}

/// CONTINUE statement
#[derive(Debug, Clone)]
pub struct ContinueStatement {
    pub label: Option<String>,
    pub when_condition: Option<Expression>,
}

/// DECLARE statement (for variable declarations)
#[derive(Debug, Clone)]
pub struct DeclareStatement {
    pub declarations: Vec<Declaration>,
}

/// RAISE statement
#[derive(Debug, Clone)]
pub struct RaiseStatement {
    pub level: RaiseLevel,
    pub condition: Option<String>, // Exception name
    pub message: Option<String>,
}

/// RAISE level
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RaiseLevel {
    Debug,
    Log,
    Info,
    Notice,
    Warning,
    Exception,
}
