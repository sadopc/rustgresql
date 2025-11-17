//! SQL lexer
//!
//! Tokenizes SQL input into meaningful tokens for parsing

use crate::Result;

/// Token type
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Keywords
    Select,
    Insert,
    Update,
    Delete,
    Create,
    Table,
    Index,
    From,
    Where,
    Into,
    Values,
    Primary,
    Key,
    Not,
    Null,
    And,
    Or,
    Order,
    By,
    Group,
    Having,
    Join,
    Inner,
    Left,
    Right,
    Full,
    Outer,
    Anti,
    Semi,
    On,
    As,
    Distinct,
    Count,
    Sum,
    Avg,
    Min,
    Max,
    Set,
    Drop,
    Alter,
    References,
    If,
    Exists,
    Default,
    Check,
    Constraint,
    Foreign,
    Add,
    Column,
    Rename,
    To,
    Union,
    Intersect,
    Except,
    All,
    Unique,
    Over,
    Partition,
    Window,
    Rows,
    Range,
    Between,
    Unbounded,
    Preceding,
    Following,
    Current,
    With,
    Recursive,
    View,
    Materialized,
    Refresh,
    Concurrently,
    Cascade,
    Data,

    // Stored Procedure and Control Flow Keywords
    Procedure,
    Function,
    Language,
    Begin,
    End,
    Declare,
    Loop,
    While,
    For,
    Then,
    Else,
    Case,
    When,
    Return,
    Exit,
    Continue,
    Perform,
    Raise,
    Exception,
    Replace,
    Definer,
    Of,
    Call,
    Security,

    // Operators
    Equals,
    NotEquals,
    LessThan,
    LessThanOrEquals,
    GreaterThan,
    GreaterThanOrEquals,
    Like,
    ILike,
    In,
    Is,
    Plus,
    Minus,
    Divide,

    // Punctuation
    LeftParen,
    RightParen,
    Comma,
    Semicolon,
    Dot,
    Asterisk,

    // Literals
    Identifier(String),
    String(String),
    Number(String),

    // Special
    Whitespace,
    Comment(String),
    EOF,
}

/// Token with position information
#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub line: usize,
    pub column: usize,
    pub value: String,
}

impl Token {
    /// Create a new token
    pub fn new(token_type: TokenType, line: usize, column: usize, value: String) -> Self {
        Self {
            token_type,
            line,
            column,
            value,
        }
    }

    /// Get the token value as string
    pub fn as_string(&self) -> &str {
        &self.value
    }

    /// Check if token is a keyword
    pub fn is_keyword(&self) -> bool {
        matches!(
            self.token_type,
            TokenType::Select
                | TokenType::Insert
                | TokenType::Update
                | TokenType::Delete
                | TokenType::Create
                | TokenType::Table
                | TokenType::Index
                | TokenType::From
                | TokenType::Where
                | TokenType::Into
                | TokenType::Values
                | TokenType::Primary
                | TokenType::Key
                | TokenType::Not
                | TokenType::Null
                | TokenType::And
                | TokenType::Or
                | TokenType::Order
                | TokenType::By
                | TokenType::Group
                | TokenType::Having
                | TokenType::Join
                | TokenType::Inner
                | TokenType::Left
                | TokenType::Right
                | TokenType::Outer
                | TokenType::On
                | TokenType::As
                | TokenType::Distinct
                | TokenType::Count
                | TokenType::Sum
                | TokenType::Avg
                | TokenType::Min
                | TokenType::Max
                | TokenType::Set
                | TokenType::Drop
                | TokenType::Alter
                | TokenType::References
                | TokenType::If
                | TokenType::Exists
                | TokenType::Default
                | TokenType::Check
                | TokenType::Unique
                | TokenType::With
                | TokenType::Recursive
                | TokenType::View
                | TokenType::Materialized
                | TokenType::Refresh
                | TokenType::Concurrently
                | TokenType::Cascade
                | TokenType::Data
                | TokenType::Procedure
                | TokenType::Function
                | TokenType::Language
                | TokenType::Begin
                | TokenType::End
                | TokenType::Declare
                | TokenType::Loop
                | TokenType::While
                | TokenType::For
                | TokenType::Then
                | TokenType::Else
                | TokenType::Case
                | TokenType::When
                | TokenType::Return
                | TokenType::Exit
                | TokenType::Continue
                | TokenType::Perform
                | TokenType::Raise
                | TokenType::Exception
                | TokenType::Replace
                | TokenType::Definer
                | TokenType::Of
                | TokenType::Call
                | TokenType::Security
        )
    }
}

/// SQL lexer
pub struct Lexer {
    input: Vec<char>,
    position: usize,
    line: usize,
    column: usize,
    tokens: Vec<Token>,
}

impl Lexer {
    /// Create a new lexer for the given input
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
            tokens: Vec::new(),
        }
    }

    /// Tokenize the input into tokens
    pub fn tokenize(&mut self) -> Result<Vec<Token>> {
        while !self.is_at_end() {
            let current_char = self.current_char();

            // Skip whitespace
            if current_char.is_whitespace() {
                self.consume_whitespace();
                continue;
            }

            // Handle comments
            if current_char == '-' && self.peek_char() == Some('-') {
                self.consume_line_comment();
                continue;
            }

            // Handle string literals
            if current_char == '\'' {
                self.consume_string()?;
                continue;
            }

            // Handle identifiers and keywords
            if current_char.is_alphabetic() || current_char == '_' {
                self.consume_identifier_or_keyword();
                continue;
            }

            // Handle numbers
            if current_char.is_numeric() {
                self.consume_number();
                continue;
            }

            // Handle operators
            if current_char == '=' {
                self.add_token(TokenType::Equals, 1);
                continue;
            }

            if current_char == '+' {
                self.add_token(TokenType::Plus, 1);
                continue;
            }

            if current_char == '-' {
                // Check if this is part of a comment
                if self.peek_char() == Some('-') {
                    self.consume_line_comment();
                    continue;
                } else {
                    self.add_token(TokenType::Minus, 1);
                    continue;
                }
            }

            if current_char == '/' {
                self.add_token(TokenType::Divide, 1);
                continue;
            }

            if current_char == '!' && self.peek_char() == Some('=') {
                self.add_token(TokenType::NotEquals, 2);
                continue;
            }

            if current_char == '<' {
                if self.peek_char() == Some('=') {
                    self.add_token(TokenType::LessThanOrEquals, 2);
                } else if self.peek_char() == Some('>') {
                    self.add_token(TokenType::NotEquals, 2);
                } else {
                    self.add_token(TokenType::LessThan, 1);
                }
                continue;
            }

            if current_char == '>' {
                if self.peek_char() == Some('=') {
                    self.add_token(TokenType::GreaterThanOrEquals, 2);
                } else {
                    self.add_token(TokenType::GreaterThan, 1);
                }
                continue;
            }

            // Handle punctuation
            match current_char {
                '(' => self.add_token(TokenType::LeftParen, 1),
                ')' => self.add_token(TokenType::RightParen, 1),
                ',' => self.add_token(TokenType::Comma, 1),
                ';' => self.add_token(TokenType::Semicolon, 1),
                '.' => self.add_token(TokenType::Dot, 1),
                '*' => self.add_token(TokenType::Asterisk, 1),
                _ => {
                    return Err(crate::error::RustgreSQLError::Parse(
                        format!("Unexpected character '{}' at line {}, column {}",
                               current_char, self.line, self.column)
                    ));
                }
            }
        }

        // Add EOF token
        self.tokens.push(Token::new(TokenType::EOF, self.line, self.column, String::new()));

        Ok(self.tokens.clone())
    }

    /// Get the current character
    fn current_char(&self) -> char {
        self.input[self.position]
    }

    /// Peek at the next character without consuming
    fn peek_char(&self) -> Option<char> {
        if self.position + 1 < self.input.len() {
            Some(self.input[self.position + 1])
        } else {
            None
        }
    }

    /// Check if at end of input
    fn is_at_end(&self) -> bool {
        self.position >= self.input.len()
    }

    /// Advance position by n characters
    fn advance(&mut self, n: usize) {
        for _ in 0..n {
            if self.is_at_end() {
                break;
            }
            if self.current_char() == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            self.position += 1;
        }
    }

    /// Consume whitespace
    fn consume_whitespace(&mut self) {
        while !self.is_at_end() && self.current_char().is_whitespace() {
            self.advance(1);
        }
    }

    /// Consume line comment (--)
    fn consume_line_comment(&mut self) {
        self.advance(2); // Skip --
        let start_line = self.line;

        while !self.is_at_end() && self.current_char() != '\n' {
            self.advance(1);
        }

        // Create comment token
        let comment: String = self.input[self.position - (self.column - 1)..self.position]
            .iter().collect();
        self.tokens.push(Token::new(
            TokenType::Comment(comment.clone()),
            start_line,
            1,
            comment,
        ));
    }

    /// Consume string literal
    fn consume_string(&mut self) -> Result<()> {
        self.advance(1); // Skip opening quote
        let start_line = self.line;
        let start_column = self.column;
        let mut string_value = String::new();

        while !self.is_at_end() {
            let current = self.current_char();

            if current == '\'' {
                // Check for escaped quote
                if self.peek_char() == Some('\'') {
                    string_value.push('\'');
                    self.advance(2); // Consume both quotes
                } else {
                    // End of string
                    self.advance(1);
                    break;
                }
            } else {
                string_value.push(current);
                self.advance(1);
            }
        }

        if self.is_at_end() {
            return Err(crate::error::RustgreSQLError::Parse(
                format!("Unterminated string starting at line {}, column {}",
                       start_line, start_column)
            ));
        }

        self.tokens.push(Token::new(
            TokenType::String(string_value.clone()),
            start_line,
            start_column,
            string_value,
        ));

        Ok(())
    }

    /// Consume identifier or keyword
    fn consume_identifier_or_keyword(&mut self) {
        let start_line = self.line;
        let start_column = self.column;
        let mut identifier = String::new();

        while !self.is_at_end() && (self.current_char().is_alphanumeric() || self.current_char() == '_') {
            identifier.push(self.current_char());
            self.advance(1);
        }

        let upper_identifier = identifier.to_uppercase();
        let token_type = match upper_identifier.as_str() {
            "SELECT" => TokenType::Select,
            "INSERT" => TokenType::Insert,
            "UPDATE" => TokenType::Update,
            "DELETE" => TokenType::Delete,
            "CREATE" => TokenType::Create,
            "TABLE" => TokenType::Table,
            "INDEX" => TokenType::Index,
            "FROM" => TokenType::From,
            "WHERE" => TokenType::Where,
            "INTO" => TokenType::Into,
            "VALUES" => TokenType::Values,
            "PRIMARY" => TokenType::Primary,
            "KEY" => TokenType::Key,
            "NOT" => TokenType::Not,
            "NULL" => TokenType::Null,
            "AND" => TokenType::And,
            "OR" => TokenType::Or,
            "ORDER" => TokenType::Order,
            "BY" => TokenType::By,
            "GROUP" => TokenType::Group,
            "HAVING" => TokenType::Having,
            "JOIN" => TokenType::Join,
            "INNER" => TokenType::Inner,
            "LEFT" => TokenType::Left,
            "RIGHT" => TokenType::Right,
            "FULL" => TokenType::Full,
            "OUTER" => TokenType::Outer,
            "ANTI" => TokenType::Anti,
            "SEMI" => TokenType::Semi,
            "ON" => TokenType::On,
            "AS" => TokenType::As,
            "DISTINCT" => TokenType::Distinct,
            "COUNT" => TokenType::Count,
            "SUM" => TokenType::Sum,
            "AVG" => TokenType::Avg,
            "MIN" => TokenType::Min,
            "MAX" => TokenType::Max,
            "LIKE" => TokenType::Like,
            "ILIKE" => TokenType::ILike,
            "IN" => TokenType::In,
            "IS" => TokenType::Is,
            "SET" => TokenType::Set,
            "DROP" => TokenType::Drop,
            "ALTER" => TokenType::Alter,
            "REFERENCES" => TokenType::References,
            "IF" => TokenType::If,
            "EXISTS" => TokenType::Exists,
            "DEFAULT" => TokenType::Default,
            "CHECK" => TokenType::Check,
            "CONSTRAINT" => TokenType::Constraint,
            "FOREIGN" => TokenType::Foreign,
            "ADD" => TokenType::Add,
            "COLUMN" => TokenType::Column,
            "RENAME" => TokenType::Rename,
            "TO" => TokenType::To,
            "UNION" => TokenType::Union,
            "INTERSECT" => TokenType::Intersect,
            "EXCEPT" => TokenType::Except,
            "ALL" => TokenType::All,
            "UNIQUE" => TokenType::Unique,
            "OVER" => TokenType::Over,
            "PARTITION" => TokenType::Partition,
            "WINDOW" => TokenType::Window,
            "ROWS" => TokenType::Rows,
            "RANGE" => TokenType::Range,
            "BETWEEN" => TokenType::Between,
            "UNBOUNDED" => TokenType::Unbounded,
            "PRECEDING" => TokenType::Preceding,
            "FOLLOWING" => TokenType::Following,
            "CURRENT" => TokenType::Current,
            "WITH" => TokenType::With,
            "RECURSIVE" => TokenType::Recursive,
            "VIEW" => TokenType::View,
            "MATERIALIZED" => TokenType::Materialized,
            "REFRESH" => TokenType::Refresh,
            "CONCURRENTLY" => TokenType::Concurrently,
            "CASCADE" => TokenType::Cascade,
            "DATA" => TokenType::Data,
            // Stored procedure and control flow keywords
            "PROCEDURE" => TokenType::Procedure,
            "FUNCTION" => TokenType::Function,
            "LANGUAGE" => TokenType::Language,
            "BEGIN" => TokenType::Begin,
            "END" => TokenType::End,
            "DECLARE" => TokenType::Declare,
            "LOOP" => TokenType::Loop,
            "WHILE" => TokenType::While,
            "FOR" => TokenType::For,
            "THEN" => TokenType::Then,
            "ELSE" => TokenType::Else,
            "CASE" => TokenType::Case,
            "WHEN" => TokenType::When,
            "RETURN" => TokenType::Return,
            "EXIT" => TokenType::Exit,
            "CONTINUE" => TokenType::Continue,
            "PERFORM" => TokenType::Perform,
            "RAISE" => TokenType::Raise,
            "EXCEPTION" => TokenType::Exception,
            "REPLACE" => TokenType::Replace,
            "DEFINER" => TokenType::Definer,
            "OF" => TokenType::Of,
            "CALL" => TokenType::Call,
            "SECURITY" => TokenType::Security,
            _ => TokenType::Identifier(identifier.clone()),
        };

        self.tokens.push(Token::new(
            token_type,
            start_line,
            start_column,
            identifier,
        ));
    }

    /// Consume number
    fn consume_number(&mut self) {
        let start_line = self.line;
        let start_column = self.column;
        let mut number = String::new();

        // Handle integer part
        while !self.is_at_end() && self.current_char().is_numeric() {
            number.push(self.current_char());
            self.advance(1);
        }

        // Handle decimal part
        if !self.is_at_end() && self.current_char() == '.' {
            number.push('.');
            self.advance(1);

            while !self.is_at_end() && self.current_char().is_numeric() {
                number.push(self.current_char());
                self.advance(1);
            }
        }

        // Handle exponent part
        if !self.is_at_end() && (self.current_char() == 'e' || self.current_char() == 'E') {
            number.push(self.current_char());
            self.advance(1);

            if !self.is_at_end() && (self.current_char() == '+' || self.current_char() == '-') {
                number.push(self.current_char());
                self.advance(1);
            }

            while !self.is_at_end() && self.current_char().is_numeric() {
                number.push(self.current_char());
                self.advance(1);
            }
        }

        self.tokens.push(Token::new(
            TokenType::Number(number.clone()),
            start_line,
            start_column,
            number,
        ));
    }

    /// Add a simple token of length 1
    fn add_token(&mut self, token_type: TokenType, length: usize) {
        let start_line = self.line;
        let start_column = self.column;
        let value: String = self.input[self.position..self.position + length].iter().collect();
        self.advance(length);
        self.tokens.push(Token::new(token_type, start_line, start_column, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let mut lexer = Lexer::new("SELECT * FROM users");
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].token_type, TokenType::Select);
        assert_eq!(tokens[1].token_type, TokenType::Asterisk);
        assert_eq!(tokens[2].token_type, TokenType::From);
        if let TokenType::Identifier(_) = tokens[3].token_type {
            // Pass
        } else {
            panic!("Expected Identifier token");
        }
        assert_eq!(tokens[4].token_type, TokenType::EOF);
    }

    #[test]
    fn test_string_literals() {
        let mut lexer = Lexer::new("INSERT INTO users (name) VALUES ('John''s')");
        let tokens = lexer.tokenize().unwrap();

        // Find the string token
        let string_tokens: Vec<_> = tokens.iter()
            .filter(|t| matches!(t.token_type, TokenType::String(_)))
            .collect();

        assert_eq!(string_tokens.len(), 1);
        assert_eq!(string_tokens[0].value, "John's");
    }

    #[test]
    fn test_numbers() {
        let mut lexer = Lexer::new("123 45.67 1.23e4");
        let tokens = lexer.tokenize().unwrap();

        let number_tokens: Vec<_> = tokens.iter()
            .filter(|t| matches!(t.token_type, TokenType::Number(_)))
            .collect();

        assert_eq!(number_tokens.len(), 3);
        assert_eq!(number_tokens[0].value, "123");
        assert_eq!(number_tokens[1].value, "45.67");
        assert_eq!(number_tokens[2].value, "1.23e4");
    }

    #[test]
    fn test_operators() {
        let mut lexer = Lexer::new("x = y AND x != y");
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[1].token_type, TokenType::Equals);
        assert_eq!(tokens[5].token_type, TokenType::And);
        assert_eq!(tokens[7].token_type, TokenType::NotEquals);
    }

    #[test]
    fn test_parentheses() {
        let mut lexer = Lexer::new("(a, b, c)");
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].token_type, TokenType::LeftParen);
        assert_eq!(tokens[2].token_type, TokenType::Comma);
        assert_eq!(tokens[4].token_type, TokenType::Comma);
        assert_eq!(tokens[6].token_type, TokenType::RightParen);
    }

    #[test]
    fn test_comments() {
        let mut lexer = Lexer::new("SELECT * -- This is a comment\nFROM users");
        let tokens = lexer.tokenize().unwrap();

        // Should have comment token
        let comment_tokens: Vec<_> = tokens.iter()
            .filter(|t| matches!(t.token_type, TokenType::Comment(_)))
            .collect();

        assert_eq!(comment_tokens.len(), 1);
        assert!(comment_tokens[0].value.contains("This is a comment"));
    }

    #[test]
    fn test_ddl_keywords() {
        let mut lexer = Lexer::new("DROP TABLE IF EXISTS users ALTER TABLE ADD REFERENCES CHECK");
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens[0].token_type, TokenType::Drop);
        assert_eq!(tokens[1].token_type, TokenType::Table);
        assert_eq!(tokens[2].token_type, TokenType::If);
        assert_eq!(tokens[3].token_type, TokenType::Exists);
        assert_eq!(tokens[4].token_type, TokenType::Identifier("users".to_string()));
        assert_eq!(tokens[5].token_type, TokenType::Alter);
        assert_eq!(tokens[6].token_type, TokenType::Table);
        assert_eq!(tokens[7].token_type, TokenType::Identifier("ADD".to_string()));
        assert_eq!(tokens[8].token_type, TokenType::References);
        assert_eq!(tokens[9].token_type, TokenType::Check);
    }
}
