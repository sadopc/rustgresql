//! SQL parser
//!
//! Consumes tokens from the lexer and builds AST structures

use crate::{Result, error::RustgreSQLError};
use super::{lexer::{Lexer, Token, TokenType}, ast::*};

/// SQL parser
pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    /// Create new parser
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    /// Parse tokens into statements
    pub fn parse(&mut self) -> Result<Vec<Statement>> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            // Skip whitespace and comments
            if let TokenType::Whitespace | TokenType::Comment(_) = self.peek().token_type {
                self.advance();
                continue;
            }

            // Parse statement
            let statement = self.parse_statement()?;
            statements.push(statement);

            // Expect semicolon or EOF
            if !self.is_at_end() && !self.match_token(TokenType::Semicolon) {
                return Err(RustgreSQLError::Parse(
                    format!("Expected semicolon at line {}, column {}",
                           self.peek().line, self.peek().column)
                ));
            }
        }

        Ok(statements)
    }

    /// Parse a single statement
    fn parse_statement(&mut self) -> Result<Statement> {
        match self.peek().token_type {
            TokenType::Select => self.parse_select(),
            TokenType::Insert => self.parse_insert(),
            TokenType::Update => self.parse_update(),
            TokenType::Delete => self.parse_delete(),
            TokenType::Create => self.parse_create(),
            TokenType::Drop => self.parse_drop(),
            TokenType::Alter => self.parse_alter(),
            _ => Err(RustgreSQLError::Parse(
                format!("Unexpected token '{}' at line {}, column {}",
                       self.peek().value, self.peek().line, self.peek().column)
            ))
        }
    }

    /// Parse SELECT statement (consumes SELECT token)
    fn parse_select(&mut self) -> Result<Statement> {
        self.consume_token(TokenType::Select, "Expected SELECT")?;
        self.parse_select_after_select()
    }

    /// Parse SELECT statement after SELECT token has been consumed
    fn parse_select_after_select(&mut self) -> Result<Statement> {

        let distinct = self.match_token(TokenType::Distinct);

        // Parse columns
        let mut columns = Vec::new();
        if self.match_token(TokenType::Asterisk) {
            columns.push(Expression::Star);
        } else {
            columns.push(self.parse_expression()?);

            while self.match_token(TokenType::Comma) {
                columns.push(self.parse_expression()?);
            }
        }

        // Parse FROM clause (optional for scalar queries)
        let mut from = Vec::new();
        if self.match_token(TokenType::From) {
            from.push(self.parse_table_ref()?);

            while self.match_token(TokenType::Comma) {
                from.push(self.parse_table_ref()?);
            }
        }

        // Parse joins
        let mut joins = Vec::new();
        while let TokenType::Join | TokenType::Left | TokenType::Right | TokenType::Inner = self.peek().token_type {
            joins.push(self.parse_join()?);
        }

        // Parse WHERE clause
        let where_clause = if self.match_token(TokenType::Where) {
            Some(self.parse_expression()?)
        } else {
            None
        };

        // Parse GROUP BY clause
        let mut group_by = Vec::new();
        if self.match_token(TokenType::Group) {
            self.consume_token(TokenType::By, "Expected BY after GROUP")?;
            group_by.push(self.parse_expression()?);
            while self.match_token(TokenType::Comma) {
                group_by.push(self.parse_expression()?);
            }
        }

        // Parse HAVING clause
        let having = if self.match_token(TokenType::Having) {
            Some(self.parse_expression()?)
        } else {
            None
        };

        // Parse ORDER BY clause
        let mut order_by = Vec::new();
        if self.match_token(TokenType::Order) {
            self.consume_token(TokenType::By, "Expected BY after ORDER")?;
            order_by.push(self.parse_order_by()?);
            while self.match_token(TokenType::Comma) {
                order_by.push(self.parse_order_by()?);
            }
        }

        // Parse LIMIT clause
        let limit = if self.match_identifier_token("LIMIT")? {
            Some(self.parse_number()?)
        } else {
            None
        };

        // Parse OFFSET clause
        let offset = if self.match_identifier_token("OFFSET")? {
            Some(self.parse_number()?)
        } else {
            None
        };

        // Parse set operations (UNION, INTERSECT, EXCEPT) after the first SELECT
          let select_statement = self.parse_set_operations(SelectStatement::Simple {
                distinct,
                columns,
                from,
                joins,
                where_clause,
                group_by,
                having,
                order_by,
                limit,
                offset,
            })?;

        Ok(Statement::Select(select_statement))
    }

    /// Parse set operations (UNION, INTERSECT, EXCEPT) with proper precedence
    /// Set operations have left-associative precedence: INTERSECT > UNION/EXCEPT
    fn parse_set_operations(&mut self, left: SelectStatement) -> Result<SelectStatement> {
        let mut result = left;

        // Check for set operations
        loop {
            let operator = if self.match_token(TokenType::Union) {
                Some(SetOperator::Union)
            } else if self.match_token(TokenType::Intersect) {
                Some(SetOperator::Intersect)
            } else if self.match_token(TokenType::Except) {
                Some(SetOperator::Except)
            } else {
                break; // No more set operations
            };

            let all = self.match_token(TokenType::All);

            // Parse the right side SELECT
            self.consume_token(TokenType::Select, "Expected SELECT after set operator")?;
            let right_select = self.parse_simple_select()?;

            // Create set operation
            let set_operation = SetOperation {
                operator: operator.unwrap(),
                left: Box::new(result),
                right: Box::new(right_select),
                all,
            };

            result = SelectStatement::SetOperation(set_operation);

            // Continue parsing for more set operations (left-associative)
        }

        Ok(result)
    }

    /// Parse a simple SELECT statement without set operations
    fn parse_simple_select(&mut self) -> Result<SelectStatement> {
        let distinct = self.match_token(TokenType::Distinct);

        // Parse columns
        let mut columns = Vec::new();
        if self.match_token(TokenType::Asterisk) {
            columns.push(Expression::Star);
        } else {
            columns.push(self.parse_expression()?);

            while self.match_token(TokenType::Comma) {
                columns.push(self.parse_expression()?);
            }
        }

        // Parse FROM clause
        self.consume_token(TokenType::From, "Expected FROM")?;
        let mut from = Vec::new();
        from.push(self.parse_table_ref()?);

        while self.match_token(TokenType::Comma) {
            from.push(self.parse_table_ref()?);
        }

        // Parse joins
        let mut joins = Vec::new();
        while let TokenType::Join | TokenType::Left | TokenType::Right | TokenType::Inner = self.peek().token_type {
            joins.push(self.parse_join()?);
        }

        // Parse WHERE clause
        let where_clause = if self.match_token(TokenType::Where) {
            Some(self.parse_expression()?)
        } else {
            None
        };

        // Parse GROUP BY clause
        let mut group_by = Vec::new();
        if self.match_token(TokenType::Group) {
            self.consume_token(TokenType::By, "Expected BY after GROUP")?;
            group_by.push(self.parse_expression()?);
            while self.match_token(TokenType::Comma) {
                group_by.push(self.parse_expression()?);
            }
        }

        // Parse HAVING clause
        let having = if self.match_token(TokenType::Having) {
            Some(self.parse_expression()?)
        } else {
            None
        };

        // Parse ORDER BY clause
        let mut order_by = Vec::new();
        if self.match_token(TokenType::Order) {
            self.consume_token(TokenType::By, "Expected BY after ORDER")?;
            order_by.push(self.parse_order_by()?);
            while self.match_token(TokenType::Comma) {
                order_by.push(self.parse_order_by()?);
            }
        }

        // Parse LIMIT clause
        let limit = if self.match_identifier_token("LIMIT")? {
            Some(self.parse_number()?)
        } else {
            None
        };

        // Parse OFFSET clause
        let offset = if self.match_identifier_token("OFFSET")? {
            Some(self.parse_number()?)
        } else {
            None
        };

        Ok(SelectStatement::Simple {
            distinct,
            columns,
            from,
            joins,
            where_clause,
            group_by,
            having,
            order_by,
            limit,
            offset,
        })
    }

    /// Parse INSERT statement
    fn parse_insert(&mut self) -> Result<Statement> {
        self.consume_token(TokenType::Insert, "Expected INSERT")?;
        self.consume_token(TokenType::Into, "Expected INTO after INSERT")?;

        let table = self.parse_table_ref()?;

        // Parse column list
        let mut columns = Vec::new();
        if self.match_token(TokenType::LeftParen) {
            columns.push(self.consume_identifier()?);
            while self.match_token(TokenType::Comma) {
                columns.push(self.consume_identifier()?);
            }
            self.consume_token(TokenType::RightParen, "Expected ')' after column list")?;
        }

        self.consume_token(TokenType::Values, "Expected VALUES")?;
        self.consume_token(TokenType::LeftParen, "Expected '(' before values")?;

        let mut values = Vec::new();
        values.push(self.parse_expression()?);
        while self.match_token(TokenType::Comma) {
            values.push(self.parse_expression()?);
        }
        self.consume_token(TokenType::RightParen, "Expected ')' after values")?;

        Ok(Statement::Insert(InsertStatement {
            table,
            columns,
            values: vec![values],
        }))
    }

    /// Parse UPDATE statement
    fn parse_update(&mut self) -> Result<Statement> {
        self.consume_token(TokenType::Update, "Expected UPDATE")?;

        let table = self.parse_table_ref()?;

        self.consume_token(TokenType::Set, "Expected SET")?;

        let mut assignments = Vec::new();

        // Parse first assignment
        let column = self.consume_identifier()?;
        self.consume_token(TokenType::Equals, "Expected '=' in assignment")?;
        let expr = self.parse_expression()?;
        assignments.push((column, expr));

        // Parse additional assignments
        while self.match_token(TokenType::Comma) {
            let column = self.consume_identifier()?;
            self.consume_token(TokenType::Equals, "Expected '=' in assignment")?;
            let expr = self.parse_expression()?;
            assignments.push((column, expr));
        }

        // Parse WHERE clause
        let where_clause = if self.match_token(TokenType::Where) {
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(Statement::Update(UpdateStatement {
            table,
            assignments,
            where_clause,
        }))
    }

    /// Parse DELETE statement
    fn parse_delete(&mut self) -> Result<Statement> {
        self.consume_token(TokenType::Delete, "Expected DELETE")?;
        self.consume_token(TokenType::From, "Expected FROM after DELETE")?;

        let table = self.parse_table_ref()?;

        // Parse WHERE clause
        let where_clause = if self.match_token(TokenType::Where) {
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(Statement::Delete(DeleteStatement {
            table,
            where_clause,
        }))
    }

    /// Parse CREATE statement
    fn parse_create(&mut self) -> Result<Statement> {
        self.consume_token(TokenType::Create, "Expected CREATE")?;

        match self.peek().token_type {
            TokenType::Table => self.parse_create_table(),
            TokenType::Index => self.parse_create_index(),
            _ => Err(RustgreSQLError::Parse(
                format!("Expected TABLE or INDEX after CREATE, got '{}' at line {}, column {}",
                       self.peek().value, self.peek().line, self.peek().column)
            ))
        }
    }

    /// Parse CREATE TABLE statement
    fn parse_create_table(&mut self) -> Result<Statement> {
        self.consume_token(TokenType::Table, "Expected TABLE")?;

        let if_not_exists = self.match_token(TokenType::If);
        if if_not_exists {
            self.consume_token(TokenType::Not, "Expected NOT after IF")?;
            self.consume_token(TokenType::Exists, "Expected EXISTS after NOT")?;
        }

        let table_name = self.consume_identifier()?;

        self.consume_token(TokenType::LeftParen, "Expected '(' after table name")?;

        let mut columns = Vec::new();
        let mut table_constraints = Vec::new();

        // Parse first column or table constraint
        let next_token = self.peek().clone();
        match next_token.token_type {
            TokenType::Primary | TokenType::Foreign | TokenType::Unique | TokenType::Check | TokenType::Constraint => {
                // It's a table constraint
                table_constraints.push(self.parse_table_constraint()?);
            }
            _ => {
                // It's a column definition
                columns.push(self.parse_column_def()?);
            }
        }

        // Parse additional columns and table constraints
        while self.match_token(TokenType::Comma) {
            let next_token = self.peek().clone();
            match next_token.token_type {
                TokenType::Primary | TokenType::Foreign | TokenType::Unique | TokenType::Check | TokenType::Constraint => {
                    // It's a table constraint
                    table_constraints.push(self.parse_table_constraint()?);
                }
                _ => {
                    // It's a column definition
                    columns.push(self.parse_column_def()?);
                }
            }
        }

        self.consume_token(TokenType::RightParen, "Expected ')' after table definition")?;

        Ok(Statement::CreateTable(CreateTableStatement {
            table_name,
            columns,
            table_constraints,
            if_not_exists,
        }))
    }

    /// Parse CREATE INDEX statement
    fn parse_create_index(&mut self) -> Result<Statement> {
        self.consume_token(TokenType::Index, "Expected INDEX")?;

        let if_not_exists = self.match_token(TokenType::If);
        if if_not_exists {
            self.consume_token(TokenType::Not, "Expected NOT after IF")?;
            self.consume_token(TokenType::Exists, "Expected EXISTS after NOT")?;
        }

        let index_name = self.consume_identifier()?;
        self.consume_token(TokenType::On, "Expected ON")?;

        let table_name = self.consume_identifier()?;
        self.consume_token(TokenType::LeftParen, "Expected '(' after table name")?;

        let mut columns = Vec::new();
        columns.push(self.consume_identifier()?);
        while self.match_token(TokenType::Comma) {
            columns.push(self.consume_identifier()?);
        }

        self.consume_token(TokenType::RightParen, "Expected ')' after index columns")?;

        Ok(Statement::CreateIndex(CreateIndexStatement {
            index_name,
            table_name,
            columns,
            unique: false, // TODO: Support UNIQUE indexes
            if_not_exists,
        }))
    }

    /// Parse DROP statement (consumes DROP token)
    fn parse_drop(&mut self) -> Result<Statement> {
        self.consume_token(TokenType::Drop, "Expected DROP")?;

        match self.peek().token_type {
            TokenType::Table => self.parse_drop_table(),
            TokenType::Index => self.parse_drop_index(),
            _ => Err(RustgreSQLError::Parse(
                format!("Expected TABLE or INDEX after DROP, got '{}'",
                       self.peek().value)
            ))
        }
    }

    /// Parse DROP TABLE statement (consumes TABLE token)
    fn parse_drop_table(&mut self) -> Result<Statement> {
        self.consume_token(TokenType::Table, "Expected TABLE")?;

        let if_exists = self.match_token(TokenType::If);
        if if_exists {
            self.consume_token(TokenType::Exists, "Expected EXISTS after IF")?;
        }

        let table_name = self.consume_identifier()?;

        Ok(Statement::DropTable(DropTableStatement {
            table_name,
            if_exists,
        }))
    }

    /// Parse DROP INDEX statement (consumes INDEX token)
    fn parse_drop_index(&mut self) -> Result<Statement> {
        self.consume_token(TokenType::Index, "Expected INDEX")?;

        let if_exists = self.match_token(TokenType::If);
        if if_exists {
            self.consume_token(TokenType::Exists, "Expected EXISTS after IF")?;
        }

        let index_name = self.consume_identifier()?;

        Ok(Statement::DropIndex(DropIndexStatement {
            index_name,
            if_exists,
        }))
    }

    /// Parse ALTER statement (consumes ALTER token)
    fn parse_alter(&mut self) -> Result<Statement> {
        self.consume_token(TokenType::Alter, "Expected ALTER")?;
        self.consume_token(TokenType::Table, "Expected TABLE after ALTER")?;

        let table_name = self.consume_identifier()?;

        // Parse the specific ALTER operation
        let operation = self.parse_alter_operation()?;

        Ok(Statement::AlterTable(AlterTableStatement {
            table_name,
            operation,
        }))
    }

    /// Parse ALTER TABLE operation
    fn parse_alter_operation(&mut self) -> Result<AlterOperation> {
        match self.peek().token_type {
            TokenType::Add => {
                self.advance();
                match self.peek().token_type {
                    TokenType::Column => self.parse_alter_add_column(),
                    TokenType::Constraint => self.parse_alter_add_constraint(),
                    _ => Err(RustgreSQLError::Parse(
                        format!("Expected COLUMN or CONSTRAINT after ADD, got '{}'",
                               self.peek().value)
                    ))
                }
            }
            TokenType::Drop => {
                self.advance();
                match self.peek().token_type {
                    TokenType::Column => self.parse_alter_drop_column(),
                    TokenType::Constraint => self.parse_alter_drop_constraint(),
                    _ => Err(RustgreSQLError::Parse(
                        format!("Expected COLUMN or CONSTRAINT after DROP, got '{}'",
                               self.peek().value)
                    ))
                }
            }
            TokenType::Rename => {
                self.advance();
                match self.peek().token_type {
                    TokenType::Column => self.parse_alter_rename_column(),
                    TokenType::To => self.parse_alter_rename_table(),
                    _ => Err(RustgreSQLError::Parse(
                        format!("Expected COLUMN or TO after RENAME, got '{}'",
                               self.peek().value)
                    ))
                }
            }
            _ => Err(RustgreSQLError::Parse(
                format!("Expected ADD, DROP, or RENAME after ALTER TABLE, got '{}'",
                       self.peek().value)
            ))
        }
    }

    /// Parse ALTER TABLE ADD COLUMN operation
    fn parse_alter_add_column(&mut self) -> Result<AlterOperation> {
        self.consume_token(TokenType::Column, "Expected COLUMN")?;
        let column = self.parse_column_def()?;
        Ok(AlterOperation::AddColumn { column })
    }

    /// Parse ALTER TABLE DROP COLUMN operation
    fn parse_alter_drop_column(&mut self) -> Result<AlterOperation> {
        self.consume_token(TokenType::Column, "Expected COLUMN")?;
        let column_name = self.consume_identifier()?;
        Ok(AlterOperation::DropColumn { column_name })
    }

    /// Parse ALTER TABLE ADD CONSTRAINT operation
    fn parse_alter_add_constraint(&mut self) -> Result<AlterOperation> {
        self.consume_token(TokenType::Constraint, "Expected CONSTRAINT")?;
        let constraint = self.parse_table_constraint()?;
        Ok(AlterOperation::AddConstraint { constraint })
    }

    /// Parse ALTER TABLE DROP CONSTRAINT operation
    fn parse_alter_drop_constraint(&mut self) -> Result<AlterOperation> {
        self.consume_token(TokenType::Constraint, "Expected CONSTRAINT")?;
        let constraint_name = self.consume_identifier()?;
        Ok(AlterOperation::DropConstraint { constraint_name })
    }

    /// Parse ALTER TABLE RENAME COLUMN operation
    fn parse_alter_rename_column(&mut self) -> Result<AlterOperation> {
        self.consume_token(TokenType::Column, "Expected COLUMN")?;
        let old_name = self.consume_identifier()?;
        self.consume_token(TokenType::To, "Expected TO")?;
        let new_name = self.consume_identifier()?;
        Ok(AlterOperation::RenameColumn { old_name, new_name })
    }

    /// Parse ALTER TABLE RENAME TO operation
    fn parse_alter_rename_table(&mut self) -> Result<AlterOperation> {
        self.consume_token(TokenType::To, "Expected TO")?;
        let new_name = self.consume_identifier()?;
        Ok(AlterOperation::RenameTable { new_name })
    }

    /// Parse column definition
    fn parse_column_def(&mut self) -> Result<ColumnDef> {
        let name = self.consume_identifier()?;
        let data_type = self.parse_data_type()?;

        let mut constraints = Vec::new();

        // Parse column constraints
        loop {
            match self.peek().token_type {
                TokenType::Not => {
                    self.advance();
                    self.consume_token(TokenType::Null, "Expected NULL after NOT")?;
                    constraints.push(ColumnConstraint::NotNull);
                }
                TokenType::Null => {
                    self.advance();
                    constraints.push(ColumnConstraint::Null);
                }
                TokenType::Default => {
                    self.advance();
                    let value = self.parse_literal_value()?;
                    // Convert value to string representation for now
                    let value_str = match &value.kind {
                        crate::types::ValueKind::String(s) => s.clone(),
                        crate::types::ValueKind::Integer(i) => i.to_string(),
                        crate::types::ValueKind::Float(f) => f.to_string(),
                        crate::types::ValueKind::Boolean(b) => b.to_string(),
                        crate::types::ValueKind::Null(_) => "NULL".to_string(),
                    };
                    constraints.push(ColumnConstraint::Default(value_str));
                }
                TokenType::Primary => {
                    self.advance();
                    self.consume_token(TokenType::Key, "Expected KEY after PRIMARY")?;
                    constraints.push(ColumnConstraint::PrimaryKey);
                }
                TokenType::Unique => {
                    self.advance();
                    constraints.push(ColumnConstraint::Unique);
                }
                TokenType::Check => {
                    self.advance();
                    self.consume_token(TokenType::LeftParen, "Expected '(' after CHECK")?;
                    let condition = self.parse_expression()?;
                    self.consume_token(TokenType::RightParen, "Expected ')' after CHECK condition")?;
                    constraints.push(ColumnConstraint::Check(condition));
                }
                TokenType::References => {
                    self.advance();
                    let ref_table = self.consume_identifier()?;
                    let mut ref_column = None;

                    if self.match_token(TokenType::LeftParen) {
                        ref_column = Some(self.consume_identifier()?);
                        self.consume_token(TokenType::RightParen, "Expected ')' after reference column")?;
                    }

                    constraints.push(ColumnConstraint::References {
                        table: ref_table,
                        column: ref_column,
                    });
                }
                _ => break,
            }
        }

        Ok(ColumnDef {
            name,
            data_type,
            constraints,
        })
    }

    /// Parse table constraint
    fn parse_table_constraint(&mut self) -> Result<TableConstraint> {
        // Check for named constraint
        let constraint_name = if self.match_token(TokenType::Constraint) {
            Some(self.consume_identifier()?)
        } else {
            None
        };

        match self.peek().token_type {
            TokenType::Primary => {
                self.advance();
                self.consume_token(TokenType::Key, "Expected KEY after PRIMARY")?;
                self.consume_token(TokenType::LeftParen, "Expected '(' after PRIMARY KEY")?;

                let mut columns = Vec::new();
                columns.push(self.consume_identifier()?);
                while self.match_token(TokenType::Comma) {
                    columns.push(self.consume_identifier()?);
                }

                self.consume_token(TokenType::RightParen, "Expected ')' after PRIMARY KEY columns")?;

                Ok(TableConstraint::PrimaryKey {
                    columns,
                    name: constraint_name,
                })
            }
            TokenType::Foreign => {
                self.advance();
                self.consume_token(TokenType::Key, "Expected KEY after FOREIGN")?;
                self.consume_token(TokenType::LeftParen, "Expected '(' after FOREIGN KEY")?;

                let mut columns = Vec::new();
                columns.push(self.consume_identifier()?);
                while self.match_token(TokenType::Comma) {
                    columns.push(self.consume_identifier()?);
                }

                self.consume_token(TokenType::RightParen, "Expected ')' after FOREIGN KEY columns")?;
                self.consume_token(TokenType::References, "Expected REFERENCES after FOREIGN KEY columns")?;

                let ref_table = self.consume_identifier()?;
                self.consume_token(TokenType::LeftParen, "Expected '(' after REFERENCES table")?;

                let mut ref_columns = Vec::new();
                ref_columns.push(self.consume_identifier()?);
                while self.match_token(TokenType::Comma) {
                    ref_columns.push(self.consume_identifier()?);
                }

                self.consume_token(TokenType::RightParen, "Expected ')' after REFERENCES columns")?;

                Ok(TableConstraint::ForeignKey {
                    columns,
                    ref_table,
                    ref_columns,
                    name: constraint_name,
                })
            }
            TokenType::Unique => {
                self.advance();
                self.consume_token(TokenType::LeftParen, "Expected '(' after UNIQUE")?;

                let mut columns = Vec::new();
                columns.push(self.consume_identifier()?);
                while self.match_token(TokenType::Comma) {
                    columns.push(self.consume_identifier()?);
                }

                self.consume_token(TokenType::RightParen, "Expected ')' after UNIQUE columns")?;

                Ok(TableConstraint::Unique {
                    columns,
                    name: constraint_name,
                })
            }
            TokenType::Check => {
                self.advance();
                self.consume_token(TokenType::LeftParen, "Expected '(' after CHECK")?;
                let condition = self.parse_expression()?;
                self.consume_token(TokenType::RightParen, "Expected ')' after CHECK condition")?;

                Ok(TableConstraint::Check {
                    condition,
                    name: constraint_name,
                })
            }
            _ => Err(RustgreSQLError::Parse(
                format!("Expected constraint type (PRIMARY KEY, FOREIGN KEY, UNIQUE, CHECK), got '{}'",
                       self.peek().value)
            ))
        }
    }

    /// Parse data type
    fn parse_data_type(&mut self) -> Result<crate::types::DataType> {
        let type_name = self.consume_identifier()?;
        let upper_name = type_name.to_uppercase();

        match upper_name.as_str() {
            "INTEGER" | "INT" | "INT4" => Ok(crate::types::DataType::new(crate::types::DataTypeKind::Integer)),
            "BIGINT" | "INT8" => Ok(crate::types::DataType::new(crate::types::DataTypeKind::BigInt)),
            "SMALLINT" | "INT2" => Ok(crate::types::DataType::new(crate::types::DataTypeKind::SmallInt)),
            "DECIMAL" | "NUMERIC" => {
                let mut precision = 10;
                let mut scale = 0;

                if self.match_token(TokenType::LeftParen) {
                    precision = self.parse_number()? as usize;
                    if self.match_token(TokenType::Comma) {
                        scale = self.parse_number()? as usize;
                    }
                    self.consume_token(TokenType::RightParen, "Expected ')' after decimal precision")?;
                }

                Ok(crate::types::DataType::new(crate::types::DataTypeKind::Decimal(precision, scale)))
            }
            "REAL" | "FLOAT4" => Ok(crate::types::DataType::new(crate::types::DataTypeKind::Real)),
            "DOUBLE" => {
                if self.match_identifier_token("PRECISION")? {
                    Ok(crate::types::DataType::new(crate::types::DataTypeKind::DoublePrecision))
                } else {
                    Ok(crate::types::DataType::new(crate::types::DataTypeKind::DoublePrecision))
                }
            }
            "VARCHAR" => {
                let mut length = 255;

                if self.match_token(TokenType::LeftParen) {
                    length = self.parse_number()? as usize;
                    self.consume_token(TokenType::RightParen, "Expected ')' after varchar length")?;
                }

                Ok(crate::types::DataType::new(crate::types::DataTypeKind::Varchar(length)))
            }
            "CHAR" => {
                let mut length = 1;

                if self.match_token(TokenType::LeftParen) {
                    length = self.parse_number()? as usize;
                    self.consume_token(TokenType::RightParen, "Expected ')' after char length")?;
                }

                Ok(crate::types::DataType::new(crate::types::DataTypeKind::Char(length)))
            }
            "TEXT" => Ok(crate::types::DataType::new(crate::types::DataTypeKind::Text)),
            "BOOLEAN" | "BOOL" => Ok(crate::types::DataType::new(crate::types::DataTypeKind::Boolean)),
            "DATE" => Ok(crate::types::DataType::new(crate::types::DataTypeKind::Date)),
            "TIME" => Ok(crate::types::DataType::new(crate::types::DataTypeKind::Time)),
            "TIMESTAMP" => Ok(crate::types::DataType::new(crate::types::DataTypeKind::Timestamp)),
            "INTERVAL" => Ok(crate::types::DataType::new(crate::types::DataTypeKind::Interval)),
            "BLOB" | "BYTEA" => Ok(crate::types::DataType::new(crate::types::DataTypeKind::Bytea)),
            _ => Err(RustgreSQLError::Parse(
                format!("Unknown data type: {}", type_name)
            ))
        }
    }

    /// Parse table reference
    fn parse_table_ref(&mut self) -> Result<TableRef> {
        let name = self.consume_identifier()?;
        let mut alias = None;

        if self.match_token(TokenType::As) {
            alias = Some(self.consume_identifier()?);
        } else if let TokenType::Identifier(_) = self.peek().token_type {
            // Check if next token is an alias (not a keyword)
            let next_token = self.peek();
            if !next_token.is_keyword() {
                alias = Some(self.consume_identifier()?);
            }
        }

        Ok(TableRef { name, alias })
    }

    /// Parse join clause
    fn parse_join(&mut self) -> Result<JoinCondition> {
        let join_type = if self.match_token(TokenType::Left) {
            if self.match_token(TokenType::Outer) {
                JoinType::Left
            } else if self.match_token(TokenType::Anti) {
                JoinType::LeftAnti
            } else if self.match_token(TokenType::Semi) {
                JoinType::LeftSemi
            } else {
                JoinType::Left
            }
        } else if self.match_token(TokenType::Right) {
            if self.match_token(TokenType::Outer) {
                JoinType::Right
            } else if self.match_token(TokenType::Anti) {
                JoinType::RightAnti
            } else if self.match_token(TokenType::Semi) {
                JoinType::RightSemi
            } else {
                JoinType::Right
            }
        } else if self.match_token(TokenType::Inner) {
            JoinType::Inner
        } else if self.match_token(TokenType::Full) {
            if self.match_token(TokenType::Outer) {
                JoinType::Full
            } else {
                JoinType::Full
            }
        } else {
            self.consume_token(TokenType::Join, "Expected JOIN")?;
            JoinType::Inner
        };

        if self.peek().token_type != TokenType::Join {
            self.consume_token(TokenType::Join, "Expected JOIN")?;
        } else {
            self.advance();
        }

        let table = self.parse_table_ref()?;

        let condition = if self.match_token(TokenType::On) {
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(JoinCondition {
            table,
            join_type,
            condition,
        })
    }

    /// Parse ORDER BY expression
    fn parse_order_by(&mut self) -> Result<OrderBy> {
        let expr = self.parse_expression()?;
        let direction = if self.match_identifier_token("ASC")? {
            SortDirection::Asc
        } else if self.match_identifier_token("DESC")? {
            SortDirection::Desc
        } else {
            SortDirection::Asc // Default
        };

        Ok(OrderBy { expr, direction })
    }

    /// Parse expression
    fn parse_expression(&mut self) -> Result<Expression> {
        self.parse_or_expression()
    }

    /// Parse OR expression
    fn parse_or_expression(&mut self) -> Result<Expression> {
        let mut expr = self.parse_and_expression()?;

        while self.match_token(TokenType::Or) {
            let right = self.parse_and_expression()?;
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                op: BinaryOperator::Or,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    /// Parse AND expression
    fn parse_and_expression(&mut self) -> Result<Expression> {
        let mut expr = self.parse_not_expression()?;

        while self.match_token(TokenType::And) {
            let right = self.parse_not_expression()?;
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                op: BinaryOperator::And,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    /// Parse NOT expression
    fn parse_not_expression(&mut self) -> Result<Expression> {
        if self.match_token(TokenType::Not) {
            let expr = self.parse_not_expression()?;
            Ok(Expression::UnaryOp {
                op: UnaryOperator::Not,
                expr: Box::new(expr),
            })
        } else {
            self.parse_comparison_expression()
        }
    }

    /// Parse comparison expression
    fn parse_comparison_expression(&mut self) -> Result<Expression> {
        let mut expr = self.parse_additive_expression()?;

        while let TokenType::Equals | TokenType::NotEquals | TokenType::LessThan |
              TokenType::LessThanOrEquals | TokenType::GreaterThan |
              TokenType::GreaterThanOrEquals | TokenType::Like | TokenType::ILike |
              TokenType::In | TokenType::Is = self.peek().token_type {

            let op = match self.advance().token_type {
                TokenType::Equals => BinaryOperator::Equals,
                TokenType::NotEquals => BinaryOperator::NotEquals,
                TokenType::LessThan => BinaryOperator::LessThan,
                TokenType::LessThanOrEquals => BinaryOperator::LessThanOrEquals,
                TokenType::GreaterThan => BinaryOperator::GreaterThan,
                TokenType::GreaterThanOrEquals => BinaryOperator::GreaterThanOrEquals,
                TokenType::Like => BinaryOperator::Like,
                TokenType::ILike => BinaryOperator::ILike,
                TokenType::In => BinaryOperator::In,
                TokenType::Is => BinaryOperator::Is,
                _ => unreachable!(),
            };

            let right = self.parse_additive_expression()?;
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    /// Parse additive expression (+, -)
    fn parse_additive_expression(&mut self) -> Result<Expression> {
        let mut expr = self.parse_multiplicative_expression()?;

        while self.match_token(TokenType::Plus) || self.match_token(TokenType::Minus) {
            let op = self.previous().token_type.clone();
            let right = self.parse_multiplicative_expression()?;

            // For now, we'll treat + and - as string concatenation or numeric operations
            // This would need proper type checking in a complete implementation
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                op: if op == TokenType::Plus { BinaryOperator::Add } else { BinaryOperator::Subtract },
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    /// Parse multiplicative expression (*, /)
    fn parse_multiplicative_expression(&mut self) -> Result<Expression> {
        let mut expr = self.parse_unary_expression()?;

        while self.match_token(TokenType::Asterisk) || self.match_token(TokenType::Divide) {
            let op = if self.previous().token_type == TokenType::Asterisk {
                BinaryOperator::Multiply
            } else {
                BinaryOperator::Divide
            };

            let right = self.parse_unary_expression()?;
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                op,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    /// Parse unary expression
    fn parse_unary_expression(&mut self) -> Result<Expression> {
        if self.match_token(TokenType::Minus) {
            let expr = self.parse_unary_expression()?;
            Ok(Expression::UnaryOp {
                op: UnaryOperator::Minus,
                expr: Box::new(expr),
            })
        } else if self.match_token(TokenType::Plus) {
            let expr = self.parse_unary_expression()?;
            Ok(Expression::UnaryOp {
                op: UnaryOperator::Plus,
                expr: Box::new(expr),
            })
        } else {
            self.parse_primary_expression()
        }
    }

    /// Parse primary expression
    fn parse_primary_expression(&mut self) -> Result<Expression> {
        match &self.peek().token_type {
            TokenType::String(_) => {
                let value = self.advance().value.clone();
                Ok(Expression::Value(crate::types::Value {
                    kind: crate::types::ValueKind::String(value),
                }))
            }
            TokenType::Number(_) => {
                let value_str = self.advance().value.clone();
                if value_str.contains('.') {
                    let value: f64 = value_str.parse().unwrap();
                    Ok(Expression::Value(crate::types::Value {
                        kind: crate::types::ValueKind::Float(value),
                    }))
                } else {
                    let value: i64 = value_str.parse().unwrap();
                    Ok(Expression::Value(crate::types::Value {
                        kind: crate::types::ValueKind::Integer(value),
                    }))
                }
            }
            TokenType::Identifier(name) => {
                let name = name.clone();
                self.advance();

                // Check if it's a function call
                if self.match_token(TokenType::LeftParen) {
                    let mut args = Vec::new();
                    if !self.match_token(TokenType::RightParen) {
                        args.push(self.parse_expression()?);
                        while self.match_token(TokenType::Comma) {
                            args.push(self.parse_expression()?);
                        }
                        self.consume_token(TokenType::RightParen, "Expected ')' after function arguments")?;
                    }

                    Ok(Expression::Function { name, args })
                } else {
                    Ok(Expression::Column {
                        table: None,
                        name,
                    })
                }
            }
            TokenType::Asterisk => {
                self.advance();
                Ok(Expression::Star)
            }
            TokenType::LeftParen => {
                self.advance();
                // Check if this is a subquery (starts with SELECT)
                if self.match_token(TokenType::Select) {
                    let subquery_statement = self.parse_select_after_select()?;
                    self.consume_token(TokenType::RightParen, "Expected ')' after subquery")?;
                    Ok(Expression::Subquery(Box::new(subquery_statement)))
                } else {
                    let expr = self.parse_expression()?;
                    self.consume_token(TokenType::RightParen, "Expected ')' after expression")?;
                    Ok(expr)
                }
            }
            TokenType::Count | TokenType::Sum | TokenType::Avg | TokenType::Min | TokenType::Max => {
                let function_name = match &self.peek().token_type {
                    TokenType::Count => "COUNT",
                    TokenType::Sum => "SUM",
                    TokenType::Avg => "AVG",
                    TokenType::Min => "MIN",
                    TokenType::Max => "MAX",
                    _ => unreachable!(),
                };
                let _token = self.advance();

                // Parse function arguments if there's a left paren
                if self.match_token(TokenType::LeftParen) {
                    let mut args = Vec::new();
                    if !self.match_token(TokenType::RightParen) {
                        args.push(self.parse_expression()?);
                        while self.match_token(TokenType::Comma) {
                            args.push(self.parse_expression()?);
                        }
                        self.consume_token(TokenType::RightParen, "Expected ')' after function arguments")?;
                    }
                    Ok(Expression::Function {
                        name: function_name.to_string(),
                        args,
                    })
                } else {
                    // Function without parentheses, default to star for COUNT
                    Ok(Expression::Function {
                        name: function_name.to_string(),
                        args: vec![Expression::Star],
                    })
                }
            }
            _ => Err(RustgreSQLError::Parse(
                format!("Unexpected token '{}' at line {}, column {}",
                       self.peek().value, self.peek().line, self.peek().column)
            ))
        }
    }

    /// Parse literal value for DEFAULT
    fn parse_literal_value(&mut self) -> Result<crate::types::Value> {
        let token = self.advance();
        match &token.token_type {
            TokenType::String(s) => Ok(crate::types::Value::string(s.clone())),
            TokenType::Number(n) => {
                if n.contains('.') {
                    Ok(crate::types::Value::float(n.parse().unwrap_or(0.0)))
                } else {
                    Ok(crate::types::Value::integer(n.parse().unwrap_or(0)))
                }
            }
            TokenType::Identifier(i) => Ok(crate::types::Value::string(i.clone())),
            _ => Err(RustgreSQLError::Parse(
                format!("Expected literal value at line {}, column {}",
                       token.line, token.column)
            ))
        }
    }

    /// Parse number as i64
    fn parse_number(&mut self) -> Result<i64> {
        match self.peek().token_type {
            TokenType::Number(_) => {
                let value_str = self.advance().value.clone();
                value_str.parse().map_err(|e| RustgreSQLError::Parse(
                    format!("Invalid number '{}': {}", value_str, e)
                ))
            }
            _ => Err(RustgreSQLError::Parse(
                format!("Expected number at line {}, column {}",
                       self.peek().line, self.peek().column)
            ))
        }
    }

    /// Consume identifier
    fn consume_identifier(&mut self) -> Result<String> {
        match self.peek().token_type {
            TokenType::Identifier(ref name) => {
                let name = name.clone();
                self.advance();
                Ok(name)
            }
            _ => Err(RustgreSQLError::Parse(
                format!("Expected identifier at line {}, column {}",
                       self.peek().line, self.peek().column)
            ))
        }
    }

    /// Consume specific token
    fn consume_token(&mut self, token_type: TokenType, error_msg: &str) -> Result<()> {
        if self.peek().token_type == token_type {
            self.advance();
            Ok(())
        } else {
            Err(RustgreSQLError::Parse(
                format!("{} at line {}, column {}",
                       error_msg, self.peek().line, self.peek().column)
            ))
        }
    }

    /// Check if current token matches type
    fn match_token(&mut self, token_type: TokenType) -> bool {
        if self.peek().token_type == token_type {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Check if current token matches identifier
    fn match_identifier_token(&mut self, identifier: &str) -> Result<bool> {
        let token_type = self.peek().token_type.clone();
        match token_type {
            TokenType::Identifier(id) if id.to_uppercase() == identifier => {
                self.advance();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Get current token
    fn peek(&self) -> &Token {
        if self.is_at_end() {
            &self.tokens[self.tokens.len() - 1] // Return EOF token
        } else {
            &self.tokens[self.current]
        }
    }

    /// Get previous token
    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }

    /// Check if at end of tokens
    fn is_at_end(&self) -> bool {
        self.current >= self.tokens.len() ||
        self.tokens[self.current].token_type == TokenType::EOF
    }

    /// Advance to next token
    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        &self.tokens[self.current - 1]
    }
}

/// Parse SQL string into AST
pub fn parse_sql(sql: &str) -> Result<Vec<Statement>> {
    let mut lexer = Lexer::new(sql);
    let tokens = lexer.tokenize()?;

    let mut parser = Parser::new(tokens);
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_select() -> Result<()> {
        let sql = "SELECT name, age FROM users;";
        let statements = parse_sql(sql)?;

        assert_eq!(statements.len(), 1);

        if let Statement::Select(select) = &statements[0] {
            if let SelectStatement::Simple { from, columns, .. } = select {
                assert_eq!(from.len(), 1);
                assert_eq!(from[0].name, "users");
                assert_eq!(columns.len(), 2);

                if let Expression::Column { name, .. } = &columns[0] {
                    assert_eq!(name, "name");
                } else {
                    panic!("Expected column expression");
                }
            } else {
                panic!("Expected simple SELECT statement");
            }
        } else {
            panic!("Expected SELECT statement");
        }

        Ok(())
    }

    #[test]
    fn test_select_with_where() -> Result<()> {
        let sql = "SELECT * FROM users WHERE age > 18;";
        let statements = parse_sql(sql)?;

        assert_eq!(statements.len(), 1);

        if let Statement::Select(select) = &statements[0] {
            if let SelectStatement::Simple { where_clause, .. } = select {
                assert!(where_clause.is_some());
            } else {
                panic!("Expected simple SELECT statement");
            }
        } else {
            panic!("Expected SELECT statement");
        }

        Ok(())
    }

    #[test]
    fn test_create_table() -> Result<()> {
        let sql = "CREATE TABLE users (id INT PRIMARY KEY, name VARCHAR(255), age INTEGER);";
        let statements = parse_sql(sql)?;

        assert_eq!(statements.len(), 1);

        if let Statement::CreateTable(create) = &statements[0] {
            assert_eq!(create.table_name, "users");
            assert_eq!(create.columns.len(), 3);
            assert_eq!(create.table_constraints.len(), 0);

            // Check first column (id) has PRIMARY KEY constraint
            let id_column = &create.columns[0];
            assert_eq!(id_column.name, "id");
            assert!(id_column.constraints.iter().any(|c| matches!(c, ColumnConstraint::PrimaryKey)));
        } else {
            panic!("Expected CREATE TABLE statement");
        }

        Ok(())
    }

    #[test]
    fn test_insert_statement() -> Result<()> {
        let sql = "INSERT INTO users (name, age) VALUES ('John', 25);";
        let statements = parse_sql(sql)?;

        assert_eq!(statements.len(), 1);

        if let Statement::Insert(insert) = &statements[0] {
            assert_eq!(insert.table.name, "users");
            assert_eq!(insert.columns, vec!["name", "age"]);
            assert_eq!(insert.values.len(), 1);
            assert_eq!(insert.values[0].len(), 2);
        } else {
            panic!("Expected INSERT statement");
        }

        Ok(())
    }

    #[test]
    fn test_drop_table() -> Result<()> {
        let sql = "DROP TABLE users;";
        let statements = parse_sql(sql)?;

        assert_eq!(statements.len(), 1);

        if let Statement::DropTable(drop_table) = &statements[0] {
            assert_eq!(drop_table.table_name, "users");
            assert_eq!(drop_table.if_exists, false);
        } else {
            panic!("Expected DROP TABLE statement");
        }

        Ok(())
    }

    #[test]
    fn test_drop_table_if_exists() -> Result<()> {
        let sql = "DROP TABLE IF EXISTS users;";
        let statements = parse_sql(sql)?;

        assert_eq!(statements.len(), 1);

        if let Statement::DropTable(drop_table) = &statements[0] {
            assert_eq!(drop_table.table_name, "users");
            assert_eq!(drop_table.if_exists, true);
        } else {
            panic!("Expected DROP TABLE IF EXISTS statement");
        }

        Ok(())
    }

    #[test]
    fn test_alter_table_add_column() -> Result<()> {
        let sql = "ALTER TABLE users ADD COLUMN email VARCHAR(255);";
        let statements = parse_sql(sql)?;

        assert_eq!(statements.len(), 1);

        if let Statement::AlterTable(alter_table) = &statements[0] {
            assert_eq!(alter_table.table_name, "users");
            match &alter_table.operation {
                AlterOperation::AddColumn { column } => {
                    assert_eq!(column.name, "email");
                    assert_eq!(column.constraints.len(), 0);
                }
                _ => panic!("Expected ADD COLUMN operation"),
            }
        } else {
            panic!("Expected ALTER TABLE statement");
        }

        Ok(())
    }

    #[test]
    fn test_create_table_with_constraints() -> Result<()> {
        let sql = "CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            email VARCHAR(255) NOT NULL UNIQUE,
            age INTEGER CHECK (age > 0)
        );";
        let statements = parse_sql(sql)?;

        assert_eq!(statements.len(), 1);

        if let Statement::CreateTable(create_table) = &statements[0] {
            assert_eq!(create_table.table_name, "users");
            assert_eq!(create_table.columns.len(), 3);

            // Check id column has PRIMARY KEY constraint
            let id_column = &create_table.columns[0];
            assert_eq!(id_column.name, "id");
            assert!(id_column.constraints.iter().any(|c| matches!(c, ColumnConstraint::PrimaryKey)));

            // Check email column has NOT NULL and UNIQUE constraints
            let email_column = &create_table.columns[1];
            assert_eq!(email_column.name, "email");
            assert!(email_column.constraints.iter().any(|c| matches!(c, ColumnConstraint::NotNull)));
            assert!(email_column.constraints.iter().any(|c| matches!(c, ColumnConstraint::Unique)));

            // Check age column has CHECK constraint
            let age_column = &create_table.columns[2];
            assert_eq!(age_column.name, "age");
            assert!(age_column.constraints.iter().any(|c| matches!(c, ColumnConstraint::Check(_))));
        } else {
            panic!("Expected CREATE TABLE statement");
        }

        Ok(())
    }

    #[test]
    fn test_update_statement() -> Result<()> {
        let sql = "UPDATE users SET age = 26 WHERE name = 'John';";
        let statements = parse_sql(sql)?;

        assert_eq!(statements.len(), 1);

        if let Statement::Update(update) = &statements[0] {
            assert_eq!(update.table.name, "users");
            assert_eq!(update.assignments.len(), 1);
            assert_eq!(update.assignments[0].0, "age");
        } else {
            panic!("Expected UPDATE statement");
        }

        Ok(())
    }

    #[test]
    fn test_delete_statement() -> Result<()> {
        let sql = "DELETE FROM users WHERE age < 18;";
        let statements = parse_sql(sql)?;

        assert_eq!(statements.len(), 1);

        if let Statement::Delete(delete) = &statements[0] {
            assert_eq!(delete.table.name, "users");
            assert!(delete.where_clause.is_some());
        } else {
            panic!("Expected DELETE statement");
        }

        Ok(())
    }
}
