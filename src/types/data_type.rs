//! Data type definitions

use crate::Result;
use std::fmt;

/// Data type kind with PostgreSQL compatibility
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DataTypeKind {
    // Integer types
    SmallInt,      // int2
    Integer,       // int4
    BigInt,        // int8

    // Floating point types
    Real,          // float4
    DoublePrecision, // float8
    Numeric(usize, usize), // Numeric(precision, scale)
    Decimal(usize, usize), // Decimal(precision, scale)

    // Character types
    Char(usize),   // char(n)
    Varchar(usize), // varchar(n)
    Text,          // text

    // Binary types
    Bytea,         // bytea

    // Date/Time types
    Date,          // date
    Time,          // time without time zone
    TimeWithTimeZone, // time with time zone
    Timestamp,     // timestamp without time zone
    TimestampWithTimeZone, // timestamp with time zone
    Interval,      // interval

    // Boolean type
    Boolean,       // bool

    // Network address types
    Inet,          // inet
    Cidr,          // cidr
    MacAddr,       // macaddr
    MacAddr8,      // macaddr8

    // UUID type
    Uuid,          // uuid

    // JSON types
    Json,          // json
    JsonB,         // jsonb

    // Array type (for any element type)
    Array(Box<DataTypeKind>),

    // Other types
    Serial,        // serial (auto-incrementing integer)
    BigSerial,     // bigserial (auto-incrementing bigint)
}

/// Data type with nullability and other constraints
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DataType {
    pub kind: DataTypeKind,
    pub nullable: bool,
    pub has_default: bool,
    pub default_value: Option<String>,
    pub collation: Option<String>,
}

impl DataType {
    /// Create a new data type
    pub fn new(kind: DataTypeKind) -> Self {
        Self {
            kind,
            nullable: true,
            has_default: false,
            default_value: None,
            collation: None,
        }
    }

    /// Set nullable constraint
    pub fn nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    /// Set default value
    pub fn default_value(mut self, value: String) -> Self {
        self.has_default = true;
        self.default_value = Some(value);
        self
    }

    /// Set collation
    pub fn collation(mut self, collation: String) -> Self {
        self.collation = Some(collation);
        self
    }

    /// Get type name for SQL
    pub fn type_name(&self) -> String {
        match &self.kind {
            DataTypeKind::SmallInt => "smallint".to_string(),
            DataTypeKind::Integer => "integer".to_string(),
            DataTypeKind::BigInt => "bigint".to_string(),
            DataTypeKind::Real => "real".to_string(),
            DataTypeKind::DoublePrecision => "double precision".to_string(),
            DataTypeKind::Numeric(p, s) => format!("numeric({}, {})", p, s),
            DataTypeKind::Decimal(p, s) => format!("decimal({}, {})", p, s),
            DataTypeKind::Char(n) => format!("char({})", n),
            DataTypeKind::Varchar(n) => format!("varchar({})", n),
            DataTypeKind::Text => "text".to_string(),
            DataTypeKind::Bytea => "bytea".to_string(),
            DataTypeKind::Date => "date".to_string(),
            DataTypeKind::Time => "time".to_string(),
            DataTypeKind::TimeWithTimeZone => "time with time zone".to_string(),
            DataTypeKind::Timestamp => "timestamp".to_string(),
            DataTypeKind::TimestampWithTimeZone => "timestamp with time zone".to_string(),
            DataTypeKind::Interval => "interval".to_string(),
            DataTypeKind::Boolean => "boolean".to_string(),
            DataTypeKind::Inet => "inet".to_string(),
            DataTypeKind::Cidr => "cidr".to_string(),
            DataTypeKind::MacAddr => "macaddr".to_string(),
            DataTypeKind::MacAddr8 => "macaddr8".to_string(),
            DataTypeKind::Uuid => "uuid".to_string(),
            DataTypeKind::Json => "json".to_string(),
            DataTypeKind::JsonB => "jsonb".to_string(),
            DataTypeKind::Array(element_type) => {
                // Get type name for the element type
                let temp_dt = DataType::new(*element_type.clone());
                format!("{}[]", temp_dt.type_name())
            }
            DataTypeKind::Serial => "serial".to_string(),
            DataTypeKind::BigSerial => "bigserial".to_string(),
        }
    }

    /// Get storage size in bytes (approximate)
    pub fn storage_size(&self) -> usize {
        match &self.kind {
            DataTypeKind::SmallInt => 2,
            DataTypeKind::Integer | DataTypeKind::Serial => 4,
            DataTypeKind::BigInt | DataTypeKind::BigSerial => 8,
            DataTypeKind::Real => 4,
            DataTypeKind::DoublePrecision => 8,
            DataTypeKind::Numeric(_, _) => 16, // Variable, approximate
            DataTypeKind::Decimal(_, _) => 16, // Variable, approximate
            DataTypeKind::Char(n) => *n,
            DataTypeKind::Varchar(n) => 4 + *n, // 4 byte header + data
            DataTypeKind::Text => 64, // Variable, approximate
            DataTypeKind::Bytea => 64, // Variable, approximate
            DataTypeKind::Date => 4,
            DataTypeKind::Time => 8,
            DataTypeKind::TimeWithTimeZone => 12,
            DataTypeKind::Timestamp => 8,
            DataTypeKind::TimestampWithTimeZone => 12,
            DataTypeKind::Interval => 16,
            DataTypeKind::Boolean => 1,
            DataTypeKind::Inet => 12,
            DataTypeKind::Cidr => 12,
            DataTypeKind::MacAddr => 6,
            DataTypeKind::MacAddr8 => 8,
            DataTypeKind::Uuid => 16,
            DataTypeKind::Json => 64, // Variable, approximate
            DataTypeKind::JsonB => 64, // Variable, approximate
            DataTypeKind::Array(_) => 24, // Array header, elements stored separately
        }
    }

    /// Check if this is a numeric type
    pub fn is_numeric(&self) -> bool {
        matches!(&self.kind,
            DataTypeKind::SmallInt | DataTypeKind::Integer | DataTypeKind::BigInt |
            DataTypeKind::Real | DataTypeKind::DoublePrecision |
            DataTypeKind::Numeric(_, _) | DataTypeKind::Decimal(_, _) |
            DataTypeKind::Serial | DataTypeKind::BigSerial
        )
    }

    /// Check if this is a character type
    pub fn is_character(&self) -> bool {
        matches!(&self.kind,
            DataTypeKind::Char(_) | DataTypeKind::Varchar(_) | DataTypeKind::Text
        )
    }

    /// Check if this is a temporal type
    pub fn is_temporal(&self) -> bool {
        matches!(&self.kind,
            DataTypeKind::Date | DataTypeKind::Time | DataTypeKind::TimeWithTimeZone |
            DataTypeKind::Timestamp | DataTypeKind::TimestampWithTimeZone | DataTypeKind::Interval
        )
    }

    /// Check if this is a type that supports ordering
    pub fn is_orderable(&self) -> bool {
        !matches!(&self.kind,
            DataTypeKind::Json | DataTypeKind::Array(_) | DataTypeKind::Bytea
        )
    }
}

/// Parse data type from SQL string
pub fn parse_data_type(sql_type: &str) -> Result<DataType> {
    let binding = sql_type.to_lowercase();
    let lower_type = binding.trim();

    // Check for array suffix (e.g., "integer[]", "text[]")
    let is_array = lower_type.ends_with("[]");
    let base_type_str = if is_array {
        &lower_type[..lower_type.len() - 2]
    } else {
        lower_type
    };

    // Parse the base type
    let base_type = match base_type_str {
        "smallint" | "int2" => DataType::new(DataTypeKind::SmallInt),
        "integer" | "int" | "int4" => DataType::new(DataTypeKind::Integer),
        "bigint" | "int8" => DataType::new(DataTypeKind::BigInt),
        "real" | "float4" => DataType::new(DataTypeKind::Real),
        "double precision" | "float8" => DataType::new(DataTypeKind::DoublePrecision),
        "boolean" | "bool" => DataType::new(DataTypeKind::Boolean),
        "text" => DataType::new(DataTypeKind::Text),
        "bytea" => DataType::new(DataTypeKind::Bytea),
        "date" => DataType::new(DataTypeKind::Date),
        "time" => DataType::new(DataTypeKind::Time),
        "time with time zone" | "timetz" => DataType::new(DataTypeKind::TimeWithTimeZone),
        "timestamp" => DataType::new(DataTypeKind::Timestamp),
        "timestamp with time zone" | "timestamptz" => DataType::new(DataTypeKind::TimestampWithTimeZone),
        "interval" => DataType::new(DataTypeKind::Interval),
        "uuid" => DataType::new(DataTypeKind::Uuid),
        "json" => DataType::new(DataTypeKind::Json),
        "jsonb" => DataType::new(DataTypeKind::JsonB),
        "serial" => DataType::new(DataTypeKind::Serial),
        "bigserial" => DataType::new(DataTypeKind::BigSerial),
        "inet" => DataType::new(DataTypeKind::Inet),
        "cidr" => DataType::new(DataTypeKind::Cidr),
        "macaddr" => DataType::new(DataTypeKind::MacAddr),
        "macaddr8" => DataType::new(DataTypeKind::MacAddr8),

        // Types with parameters
        _ if base_type_str.starts_with("varchar(") => {
            if let Some(start) = base_type_str.find('(') {
                if let Some(end) = base_type_str.find(')') {
                    let len_str = &base_type_str[start+1..end];
                    if let Ok(len) = len_str.parse::<usize>() {
                        DataType::new(DataTypeKind::Varchar(len))
                    } else {
                        return Err(crate::error::RustgreSQLError::Parse(format!("Invalid varchar length: {}", len_str)));
                    }
                } else {
                    return Err(crate::error::RustgreSQLError::Parse("Unclosed varchar parenthesis".to_string()));
                }
            } else {
                return Err(crate::error::RustgreSQLError::Parse("Invalid varchar syntax".to_string()));
            }
        }

        _ if base_type_str.starts_with("char(") => {
            if let Some(start) = base_type_str.find('(') {
                if let Some(end) = base_type_str.find(')') {
                    let len_str = &base_type_str[start+1..end];
                    if let Ok(len) = len_str.parse::<usize>() {
                        DataType::new(DataTypeKind::Char(len))
                    } else {
                        return Err(crate::error::RustgreSQLError::Parse(format!("Invalid char length: {}", len_str)));
                    }
                } else {
                    return Err(crate::error::RustgreSQLError::Parse("Unclosed char parenthesis".to_string()));
                }
            } else {
                return Err(crate::error::RustgreSQLError::Parse("Invalid char syntax".to_string()));
            }
        }

        _ if base_type_str.starts_with("numeric(") => {
            // Parse numeric(precision, scale)
            if let Some(start) = base_type_str.find('(') {
                if let Some(end) = base_type_str.find(')') {
                    let params = &base_type_str[start+1..end];
                    let parts: Vec<&str> = params.split(',').collect();
                    if parts.len() == 2 {
                        if let (Ok(precision), Ok(scale)) = (parts[0].trim().parse::<usize>(), parts[1].trim().parse::<usize>()) {
                            DataType::new(DataTypeKind::Numeric(precision, scale))
                        } else {
                            return Err(crate::error::RustgreSQLError::Parse(format!("Invalid numeric parameters: {}", params)));
                        }
                    } else {
                        return Err(crate::error::RustgreSQLError::Parse("Numeric requires precision and scale".to_string()));
                    }
                } else {
                    return Err(crate::error::RustgreSQLError::Parse("Unclosed numeric parenthesis".to_string()));
                }
            } else {
                return Err(crate::error::RustgreSQLError::Parse("Invalid numeric syntax".to_string()));
            }
        }

        _ => return Err(crate::error::RustgreSQLError::Parse(format!("Unknown data type: {}", base_type_str)))
    };

    // If it was an array type, wrap it
    if is_array {
        Ok(DataType::new(DataTypeKind::Array(Box::new(base_type.kind))))
    } else {
        Ok(base_type)
    }
}

impl fmt::Display for DataTypeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataTypeKind::SmallInt => write!(f, "smallint"),
            DataTypeKind::Integer => write!(f, "integer"),
            DataTypeKind::BigInt => write!(f, "bigint"),
            DataTypeKind::Real => write!(f, "real"),
            DataTypeKind::DoublePrecision => write!(f, "double precision"),
            DataTypeKind::Numeric(p, s) => write!(f, "numeric({}, {})", p, s),
            DataTypeKind::Decimal(p, s) => write!(f, "decimal({}, {})", p, s),
            DataTypeKind::Char(n) => write!(f, "char({})", n),
            DataTypeKind::Varchar(n) => write!(f, "varchar({})", n),
            DataTypeKind::Text => write!(f, "text"),
            DataTypeKind::Bytea => write!(f, "bytea"),
            DataTypeKind::Date => write!(f, "date"),
            DataTypeKind::Time => write!(f, "time"),
            DataTypeKind::TimeWithTimeZone => write!(f, "time with time zone"),
            DataTypeKind::Timestamp => write!(f, "timestamp"),
            DataTypeKind::TimestampWithTimeZone => write!(f, "timestamp with time zone"),
            DataTypeKind::Interval => write!(f, "interval"),
            DataTypeKind::Boolean => write!(f, "boolean"),
            DataTypeKind::Inet => write!(f, "inet"),
            DataTypeKind::Cidr => write!(f, "cidr"),
            DataTypeKind::MacAddr => write!(f, "macaddr"),
            DataTypeKind::MacAddr8 => write!(f, "macaddr8"),
            DataTypeKind::Uuid => write!(f, "uuid"),
            DataTypeKind::Json => write!(f, "json"),
            DataTypeKind::JsonB => write!(f, "jsonb"),
            DataTypeKind::Array(element_type) => {
                write!(f, "{}[]", element_type)
            }
            DataTypeKind::Serial => write!(f, "serial"),
            DataTypeKind::BigSerial => write!(f, "bigserial"),
        }
    }
}
