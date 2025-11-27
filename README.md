# RustgreSQL

A PostgreSQL like relational database written in Rust for educational purposes.

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)]()
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)]()

## Overview

RustgreSQL is an educational database system that implements core PostgreSQL functionality from scratch. It demonstrates database internals including parsing, query planning, execution, storage management, and transaction processing.

## Features

### SQL Support

**Data Manipulation (DML)**
- `SELECT` with `WHERE`, `ORDER BY`, `LIMIT`, `OFFSET`
- `INSERT` (single row and batch)
- `UPDATE` with `WHERE` clause
- `DELETE` with `WHERE` clause
- `DISTINCT` queries

**Joins**
- `INNER JOIN`
- `LEFT JOIN` / `RIGHT JOIN`
- `FULL OUTER JOIN`
- `CROSS JOIN`
- Self-joins
- Multi-table joins (3+ tables)

**Aggregations**
- `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`
- `COUNT(DISTINCT ...)`
- `GROUP BY` with multiple columns
- `HAVING` clause

**Subqueries**
- Scalar subqueries in SELECT
- Subqueries in WHERE with `IN`, `EXISTS`, `NOT EXISTS`
- Correlated subqueries
- Derived tables (subqueries in FROM)
- Nested subqueries

**Common Table Expressions (CTEs)**
- Simple CTEs with `WITH` clause
- Multiple CTEs in single query
- Recursive CTEs with `WITH RECURSIVE`
- CTEs with aggregations and joins

**Window Functions**
- `ROW_NUMBER()`, `RANK()`, `DENSE_RANK()`
- `LAG()`, `LEAD()`
- `SUM()`, `AVG()` as window functions
- `PARTITION BY` clause
- `ORDER BY` within windows
- `ROWS BETWEEN` frame specification

**Set Operations**
- `UNION` / `UNION ALL`
- `INTERSECT`
- `EXCEPT`

**Data Definition (DDL)**
- `CREATE TABLE` with constraints (`PRIMARY KEY`, `NOT NULL`, `DEFAULT`, `FOREIGN KEY`)
- `CREATE INDEX` / `CREATE UNIQUE INDEX`
- `CREATE VIEW` / `CREATE MATERIALIZED VIEW`
- `ALTER TABLE` (`ADD COLUMN`, `DROP COLUMN`, `RENAME COLUMN`)
- `DROP TABLE IF EXISTS`

**Data Types**
- Integer types: `SMALLINT`, `INTEGER`, `BIGINT`
- Floating point: `REAL`, `DOUBLE PRECISION`
- Exact numeric: `NUMERIC(p,s)`, `DECIMAL(p,s)`
- Character: `CHAR(n)`, `VARCHAR(n)`, `TEXT`
- Date/Time: `DATE`, `TIME`, `TIMESTAMP`
- Boolean: `BOOLEAN` (with `TRUE`/`FALSE`)
- `UUID`
- Arrays: `TEXT[]`, `INTEGER[]`

**Transactions**
- `BEGIN` / `START TRANSACTION`
- `COMMIT`
- `ROLLBACK`
- ACID compliance with MVCC

**Additional Features**
- `LIKE` pattern matching with `%` and `_` wildcards
- `IS NULL` / `IS NOT NULL`
- `CASE WHEN ... THEN ... ELSE ... END`
- `NULLS FIRST` / `NULLS LAST` in ORDER BY
- Table and column aliases
- Stored procedures and functions

### Database Internals

- **Storage Engine**: Page-based storage with 8KB pages
- **Indexing**: B-Tree indexes for efficient lookups
- **Buffer Pool**: LRU-based buffer manager with configurable pool size
- **Transactions**: MVCC (Multi-Version Concurrency Control)
- **Recovery**: Write-Ahead Logging (WAL) for crash recovery
- **Query Execution**: Volcano-style iterator model

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) 1.70 or later

### Installation

```bash
git clone https://github.com/sadopc/rustgresql.git
cd rustgresql
cargo build --release
```

### Running the REPL

```bash
cargo run --release --bin rustgresql
```

Example session:

```sql
rustgresql> CREATE TABLE users (id INTEGER PRIMARY KEY, name VARCHAR(100), email VARCHAR(100));
Query executed successfully.

rustgresql> INSERT INTO users VALUES (1, 'Alice', 'alice@example.com'), (2, 'Bob', 'bob@example.com');
Query executed successfully.

rustgresql> SELECT * FROM users;
id | name  | email
---+-------+-----------------
1  | Alice | alice@example.com
2  | Bob   | bob@example.com

rustgresql> SELECT name, ROW_NUMBER() OVER (ORDER BY id) as row_num FROM users;
name  | row_num
------+--------
Alice | 1
Bob   | 2
```

### Running the Benchmark

```bash
cargo run --release --bin benchmark
```

### Running Tests

```bash
cargo test
```

The comprehensive test suite is available at `tests/comprehensive_test_queries.sql` with 100+ queries covering all supported features.

## Performance

Benchmark results on typical hardware:

| Operation | Ops/sec | Notes |
|-----------|---------|-------|
| Simple SELECT | ~3,400 | Full table scan |
| SELECT with WHERE | ~1,470 | Filtered query |
| COUNT aggregate | ~1,750 | Single table |
| GROUP BY | ~1,530 | With aggregation |
| Single INSERT | ~1,240 | Per row |
| Batch INSERT (10 rows) | ~1,370 | Rows per second |
| UPDATE | ~800 | Single row |
| DELETE | ~3,100 | With WHERE |

## Project Structure

```
rustgresql/
├── src/
│   ├── main.rs           # REPL entry point
│   ├── lib.rs            # Library exports
│   ├── sql/
│   │   ├── lexer.rs      # SQL tokenizer
│   │   ├── parser.rs     # Recursive descent parser
│   │   └── ast.rs        # Abstract syntax tree definitions
│   ├── executor/
│   │   ├── mod.rs        # Query execution engine
│   │   ├── planner.rs    # Query planner
│   │   └── operators.rs  # Scan, Filter, Join, etc.
│   ├── storage/
│   │   ├── page.rs       # Page management
│   │   ├── btree.rs      # B-Tree implementation
│   │   └── buffer.rs     # Buffer pool manager
│   ├── catalog/          # System catalog (tables, indexes)
│   └── transaction/      # Transaction manager, WAL
├── tests/
│   └── comprehensive_test_queries.sql
└── benches/              # Performance benchmarks
```

## Future Plans

### Short Term
- [ ] Query optimizer with cost-based planning
- [ ] Hash joins for better join performance
- [ ] Parallel query execution
- [ ] More complete PostgreSQL type system
- [ ] Prepared statements

### Medium Term
- [ ] Network protocol (PostgreSQL wire protocol)
- [ ] Connection pooling
- [ ] Query result caching
- [ ] Full-text search
- [ ] JSON/JSONB data types

### Long Term
- [ ] Replication support
- [ ] Partitioned tables
- [ ] Foreign data wrappers
- [ ] Extensions system

## Known Limitations

- Join operations use nested loop algorithm (can be slow for large tables)
- No query result caching
- Single-threaded execution
- Limited error recovery in some edge cases

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/improvement`)
3. Commit your changes (`git commit -m 'Add new feature'`)
4. Push to the branch (`git push origin feature/improvement`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- PostgreSQL documentation for SQL dialect reference
- "Database Internals" by Alex Petrov
- The Rust community for excellent libraries and tooling
