use std::io::Seek;

use criterion::{criterion_group, criterion_main, Criterion};
use poe2::{bundle::Bundle, index::Index};

fn benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Reader vs Slice");
    let index = include_bytes!("../resources/_.index.bin");
    group.bench_with_input("slice", &index, |b, &f| {
        b.iter(|| Bundle::<Index>::from_slice(f));
    });

    let mut file = std::fs::File::open("./resources/_.index.bin").unwrap();
    group.bench_with_input("reader", &mut file, |b, mut f| {
        b.iter_batched(
            || {
                f.rewind().unwrap();
                f
            },
            |mut f| {
                f.rewind().unwrap();
                Bundle::<Index>::from_reader(f).unwrap();
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
