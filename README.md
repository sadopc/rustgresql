# 🦀 RustgreSQL

**Because it is a truth universally acknowledged that a single developer in possession of a good fortune (or free time) must be in want of rewriting PostgreSQL in Rust.**

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen)]()
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)]()
[![Production Ready](https://img.shields.io/badge/production--ready-absolutely_not-red)]()

Welcome to **RustgreSQL**, an educational relational database system written in Rust. It aims to be compatible with PostgreSQL's SQL dialect, supports ACID transactions, and features a specialized execution engine that is definitely not just a bunch of `if` statements in a trench coat.

## 🧐 Why?

> "I'm going to write a database from scratch," said the developer, unaware that they were about to embark on a journey of pain, suffering, and B-Tree debugging.

RustgreSQL exists to demystify the magic of database internals. It covers:
- **Parsing:** A hand-written recursive descent parser (because regex is for quitters).
- **Storage:** Page-based storage with a custom B-Tree implementation.
- **Transactions:** MVCC (Multi-Version Concurrency Control) and WAL (Write-Ahead Logging) so you can ROLLBACK your mistakes.
- **Execution:** A query engine that supports Joins, Aggregates, and even Window Functions!

## ✨ Features

It's not just a toy; it's a *complex* toy.

- **SQL Support:**
  - `SELECT` (with `JOIN`, `GROUP BY`, `HAVING`, `ORDER BY`, `LIMIT`, `OFFSET`)
  - `INSERT`, `UPDATE`, `DELETE`
  - `CREATE TABLE`, `CREATE INDEX`, `CREATE VIEW`
  - `WITH` (CTEs) and Window Functions (`OVER`, `PARTITION BY`)
  - Transactions (`BEGIN`, `COMMIT`, `ROLLBACK`)
  - Stored Procedures (`CREATE PROCEDURE`, `CREATE FUNCTION`)

- **Internals:**
  - 📦 **Buffer Pool Manager:** LRU caching for disk pages.
  - 🌲 **B-Tree Indexing:** Efficient data retrieval (O(log n) goodness).
  - 🔒 **ACID Transactions:** Serializable isolation (mostly).
  - 📝 **WAL:** Crash recovery support.

## 🚀 Getting Started

You need [Rust](https://www.rust-lang.org/) installed. Then, prepare for liftoff:

```bash
# Clone the repo
git clone https://github.com/sadopc/rustgresql.git
cd rustgresql

# Build and Run
cargo run
```

This will drop you into the **RustgreSQL REPL**.

```sql
rustgresql> CREATE TABLE users (id INTEGER, name VARCHAR(50), age INTEGER);
Query executed successfully.

rustgresql> INSERT INTO users VALUES (1, 'Ferris', 5), (2, 'Gopher', 10);
Query executed successfully.

rustgresql> SELECT * FROM users WHERE age < 10;
id | name   | age
---+--------+----
1  | Ferris | 5
```

## 🏗️ Architecture (The "Sausage Factory")

1.  **SQL Parser:** Takes your beautiful SQL and turns it into an AST (Abstract Syntax Tree). It handles everything from simple `SELECT 1` to recursive CTEs.
2.  **Query Planner:** (Currently taking a nap) - Passes the AST mostly as-is to the execution engine, but pretends to optimize it.
3.  **Execution Engine:** Iterates over the plan. It loves nested loops. It *lives* for nested loops.
4.  **Transaction Manager:** The bouncer. Ensures `COMMIT` means commit and `ROLLBACK` means "it never happened."
5.  **Storage Engine:** Manages 8KB pages on disk, organizes them into B-Trees, and hopes the OS file system cache is feeling generous.

## ⚠️ Disclaimer

**DO NOT USE THIS IN PRODUCTION.**

Unless your production environment involves storing grocery lists or fantasy football drafts that you don't mind losing. While it implements WAL and crash recovery, the "crash" part is much more tested than the "recovery" part.

## 🤝 Contributing

Found a bug? Features missing? Want to implement a Join algorithm that isn't O(n²)? Pull requests are welcome!

1.  Fork it
2.  Create your feature branch (`git checkout -b feature/amazing-optimization`)
3.  Commit your changes (`git commit -m 'Made it 1000x faster'`)
4.  Push to the branch (`git push origin feature/amazing-optimization`)
5.  Open a Pull Request

## 📜 License

MIT License. Go forth and Fork.
