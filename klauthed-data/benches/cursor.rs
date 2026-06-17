//! Micro-benchmarks for opaque pagination cursors — encoded/decoded on every
//! page boundary. Run with `cargo bench -p klauthed-data`.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use klauthed_data::pagination::Cursor;

fn bench_cursor(c: &mut Criterion) {
    // A representative keyset position: a timestamp + tiebreaker id.
    let position = (1_700_000_000_i64, 9_876_543_210_u64);

    c.bench_function("cursor_encode", |b| {
        b.iter(|| black_box(Cursor::encode(black_box(&position)).unwrap()));
    });

    let cursor = Cursor::encode(&position).unwrap();
    c.bench_function("cursor_decode", |b| {
        b.iter(|| {
            let decoded: (i64, u64) = black_box(&cursor).decode().unwrap();
            black_box(decoded)
        });
    });
}

criterion_group!(benches, bench_cursor);
criterion_main!(benches);
