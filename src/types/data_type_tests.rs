#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_type_creation() {
        let integer_type = DataType::new(DataTypeKind::Integer);
        assert_eq!(integer_type.type_name(), "integer");
        assert!(integer_type.nullable);
        assert!(!integer_type.has_default);
        assert!(integer_type.is_numeric());
        assert!(!integer_type.is_character());
        assert!(!integer_type.is_temporal());
        assert!(integer_type.is_orderable());

        let text_type = DataType::new(DataTypeKind::Text);
        assert_eq!(text_type.type_name(), "text");
        assert!(text_type.is_character());
        assert!(!text_type.is_numeric());
        assert!(text_type.is_orderable());
    }

    #[test]
    fn test_data_type_builder() {
        let varchar_type = DataType::new(DataTypeKind::Varchar(100))
            .nullable(false)
            .default_value("default_value".to_string())
            .collation("en_US".to_string());

        assert_eq!(varchar_type.type_name(), "varchar(100)");
        assert!(!varchar_type.nullable);
        assert!(varchar_type.has_default);
        assert_eq!(varchar_type.default_value, Some("default_value".to_string()));
        assert_eq!(varchar_type.collation, Some("en_US".to_string()));
    }

    #[test]
    fn test_storage_sizes() {
        let integer_type = DataType::new(DataTypeKind::Integer);
        assert_eq!(integer_type.storage_size(), 4);

        let bigint_type = DataType::new(DataTypeKind::BigInt);
        assert_eq!(bigint_type.storage_size(), 8);

        let varchar_type = DataType::new(DataTypeKind::Varchar(50));
        assert_eq!(varchar_type.storage_size(), 54); // 4 bytes header + 50 data

        let text_type = DataType::new(DataTypeKind::Text);
        assert_eq!(text_type.storage_size(), 64); // Variable, approximate

        let boolean_type = DataType::new(DataTypeKind::Boolean);
        assert_eq!(boolean_type.storage_size(), 1);
    }

    #[test]
    fn test_parse_simple_types() {
        let test_cases = vec![
            ("integer", DataTypeKind::Integer),
            ("int", DataTypeKind::Integer),
            ("int4", DataTypeKind::Integer),
            ("smallint", DataTypeKind::SmallInt),
            ("int2", DataTypeKind::SmallInt),
            ("bigint", DataTypeKind::BigInt),
            ("int8", DataTypeKind::BigInt),
            ("real", DataTypeKind::Real),
            ("float4", DataTypeKind::Real),
            ("double precision", DataTypeKind::DoublePrecision),
            ("float8", DataTypeKind::DoublePrecision),
            ("boolean", DataTypeKind::Boolean),
            ("bool", DataTypeKind::Boolean),
            ("text", DataTypeKind::Text),
            ("uuid", DataTypeKind::Uuid),
            ("json", DataTypeKind::Json),
            ("jsonb", DataTypeKind::JsonB),
            ("bytea", DataTypeKind::Bytea),
            ("date", DataTypeKind::Date),
            ("time", DataTypeKind::Time),
            ("timestamp", DataTypeKind::Timestamp),
            ("interval", DataTypeKind::Interval),
            ("serial", DataTypeKind::Serial),
            ("bigserial", DataTypeKind::BigSerial),
        ];

        for (sql_type, expected_kind) in test_cases {
            let parsed = parse_data_type(sql_type).unwrap();
            assert_eq!(parsed.kind, expected_kind, "Failed to parse: {}", sql_type);
        }
    }

    #[test]
    fn test_parse_parameterized_types() {
        // Test varchar with length
        let varchar_50 = parse_data_type("varchar(50)").unwrap();
        assert_eq!(varchar_50.kind, DataTypeKind::Varchar(50));
        assert_eq!(varchar_50.type_name(), "varchar(50)");

        // Test char with length
        let char_10 = parse_data_type("char(10)").unwrap();
        assert_eq!(char_10.kind, DataTypeKind::Char(10));
        assert_eq!(char_10.type_name(), "char(10)");

        // Test numeric with precision and scale
        let numeric_10_2 = parse_data_type("numeric(10,2)").unwrap();
        assert_eq!(numeric_10_2.kind, DataTypeKind::Numeric(10, 2));
        assert_eq!(numeric_10_2.type_name(), "numeric(10, 2)");

        let decimal_15_4 = parse_data_type("decimal(15,4)").unwrap();
        assert_eq!(decimal_15_4.kind, DataTypeKind::Decimal(15, 4));
        assert_eq!(decimal_15_4.type_name(), "decimal(15, 4)");
    }

    #[test]
    fn test_parse_case_insensitive() {
        let upper = parse_data_type("INTEGER").unwrap();
        let lower = parse_data_type("integer").unwrap();
        let mixed = parse_data_type("Integer").unwrap();

        assert_eq!(upper.kind, DataTypeKind::Integer);
        assert_eq!(lower.kind, DataTypeKind::Integer);
        assert_eq!(mixed.kind, DataTypeKind::Integer);

        let varchar_upper = parse_data_type("VARCHAR(100)").unwrap();
        let varchar_lower = parse_data_type("varchar(100)").unwrap();
        assert_eq!(varchar_upper.kind, DataTypeKind::Varchar(100));
        assert_eq!(varchar_lower.kind, DataTypeKind::Varchar(100));
    }

    #[test]
    fn test_parse_invalid_types() {
        let invalid_cases = vec![
            "unknown_type",
            "varchar", // Missing length parameter
            "varchar(", // Unclosed parenthesis
            "varchar)", // Mismatched parenthesis
            "varchar(abc)", // Invalid length
            "numeric(10)", // Missing scale parameter
            "numeric(abc,2)", // Invalid precision
            "numeric(10,def)", // Invalid scale
            "char()", // Empty length
        ];

        for invalid_type in invalid_cases {
            let result = parse_data_type(invalid_type);
            assert!(result.is_err(), "Expected error for: {}", invalid_type);
        }
    }

    #[test]
    fn test_type_categories() {
        let numeric_types = vec![
            DataTypeKind::SmallInt,
            DataTypeKind::Integer,
            DataTypeKind::BigInt,
            DataTypeKind::Real,
            DataTypeKind::DoublePrecision,
            DataTypeKind::Numeric(10, 2),
            DataTypeKind::Decimal(10, 2),
            DataTypeKind::Serial,
            DataTypeKind::BigSerial,
        ];

        for kind in numeric_types {
            let dt = DataType::new(kind);
            assert!(dt.is_numeric(), "Expected {} to be numeric", dt.type_name());
        }

        let character_types = vec![
            DataTypeKind::Char(10),
            DataTypeKind::Varchar(50),
            DataTypeKind::Text,
        ];

        for kind in character_types {
            let dt = DataType::new(kind);
            assert!(dt.is_character(), "Expected {} to be character", dt.type_name());
        }

        let temporal_types = vec![
            DataTypeKind::Date,
            DataTypeKind::Time,
            DataTypeKind::TimeWithTimeZone,
            DataTypeKind::Timestamp,
            DataTypeKind::TimestampWithTimeZone,
            DataTypeKind::Interval,
        ];

        for kind in temporal_types {
            let dt = DataType::new(kind);
            assert!(dt.is_temporal(), "Expected {} to be temporal", dt.type_name());
        }
    }

    #[test]
    fn test_orderable_types() {
        let orderable_types = vec![
            DataTypeKind::Integer,
            DataTypeKind::Text,
            DataTypeKind::Date,
            DataTypeKind::Boolean,
        ];

        for kind in orderable_types {
            let dt = DataType::new(kind);
            assert!(dt.is_orderable(), "Expected {} to be orderable", dt.type_name());
        }

        let non_orderable_types = vec![
            DataTypeKind::Json,
            DataTypeKind::Array(Box::new(DataTypeKind::Integer)),
            DataTypeKind::Bytea,
        ];

        for kind in non_orderable_types {
            let dt = DataType::new(kind);
            assert!(!dt.is_orderable(), "Expected {} to NOT be orderable", dt.type_name());
        }
    }
}