//! B-Tree benchmarks

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rustgresql::storage::{BTree, BufferPoolManager};
use std::sync::Arc;

fn create_test_btree() -> (BTree, Arc<BufferPoolManager>) {
    // This will be implemented once we have proper test fixtures
    todo!("Implement test B-Tree creation")
}

fn bench_btree_insert(c: &mut Criterion) {
    let (mut btree, _) = create_test_btree();

    c.bench_function("btree_insert", |b| {
        b.iter(|| {
            let key = format!("key_{}", black_box(1000));
            btree.insert(key.into_bytes(), black_box(42)).unwrap();
        })
    });
}

fn bench_btree_search(c: &mut Criterion) {
    let (btree, _) = create_test_btree();

    c.bench_function("btree_search", |b| {
        b.iter(|| {
            let key = b"key_500";
            btree.search(black_box(&key.to_vec())).unwrap();
        })
    });
}

criterion_group!(benches, bench_btree_insert, bench_btree_search);
criterion_main!(benches);