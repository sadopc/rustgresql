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
}

/// Column definition
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: crate::types::DataType,
    pub constraints: Vec<ColumnConstraint>,
}

/// Table reference
#[derive(Debug, Clone)]
pub struct TableRef {
    pub name: String,
    pub alias: Option<String>,
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
    },
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
    /// Subquery
    Subquery(Box<Statement>),
    /// List of expressions
    List(Vec<Expression>),
    /// Star (SELECT *)
    Star,
    /// Parameter placeholder
    Parameter(usize),
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
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum UnaryOperator {
    Not,
    Minus,
    Plus,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SortDirection {
    Asc,
    Desc,
}

/// Order by clause
#[derive(Debug, Clone)]
pub struct OrderBy {
    pub expr: Expression,
    pub direction: SortDirection,
}

/// Join type
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
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

/// SELECT statement
#[derive(Debug, Clone)]
pub enum SelectStatement {
    Simple {
        distinct: bool,
        columns: Vec<Expression>,
        from: Vec<TableRef>,
        joins: Vec<JoinCondition>,
        where_clause: Option<Expression>,
        group_by: Vec<Expression>,
        having: Option<Expression>,
        order_by: Vec<OrderBy>,
        limit: Option<i64>,
        offset: Option<i64>,
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
