//! Micro-benchmarks for the config and id hot paths exercised at startup and on
//! every request. Run with `cargo bench -p klauthed-core`.

use std::hint::black_box;
use std::str::FromStr;

use criterion::{Criterion, criterion_group, criterion_main};
use klauthed_core::config::ConfigMap;
use klauthed_core::id::Id;
use serde_json::json;

struct Thing;

fn bench_config_merge(c: &mut Criterion) {
    let base = ConfigMap::from_iter([
        (
            "database".to_owned(),
            json!({ "host": "localhost", "port": 5432, "pool": { "max": 10 } }),
        ),
        ("server".to_owned(), json!({ "bind": "0.0.0.0", "port": 8080 })),
        ("debug".to_owned(), json!(false)),
    ]);
    let overlay = ConfigMap::from_iter([
        ("database".to_owned(), json!({ "port": 6543, "user": "svc" })),
        ("debug".to_owned(), json!(true)),
        ("extra".to_owned(), json!("x")),
    ]);

    c.bench_function("config_merge", |b| {
        b.iter(|| {
            let mut merged = base.clone();
            merged.merge(black_box(overlay.clone()));
            black_box(merged)
        });
    });
}

fn bench_expand_dotted(c: &mut Criterion) {
    let flat = ConfigMap::from_iter([
        ("database.host".to_owned(), json!("localhost")),
        ("database.port".to_owned(), json!(5432)),
        ("database.pool.max".to_owned(), json!(10)),
        ("server.bind".to_owned(), json!("0.0.0.0")),
        ("server.port".to_owned(), json!(8080)),
    ]);

    c.bench_function("config_expand_dotted", |b| {
        b.iter(|| black_box(black_box(flat.clone()).expand_dotted()));
    });
}

fn bench_id(c: &mut Criterion) {
    c.bench_function("id_new_v7", |b| b.iter(|| black_box(Id::<Thing>::new_v7())));

    let text = Id::<Thing>::new_v7().to_string();
    c.bench_function("id_parse", |b| {
        b.iter(|| black_box(Id::<Thing>::from_str(black_box(&text)).unwrap()));
    });
}

criterion_group!(benches, bench_config_merge, bench_expand_dotted, bench_id);
criterion_main!(benches);
