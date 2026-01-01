//! Performance benchmarks for proxy operations

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use mytunnel_server::pool::{BufferPool, BufferSize};

fn buffer_pool_benchmark(c: &mut Criterion) {
    let pool = BufferPool::new(1000, 500, 100);

    let mut group = c.benchmark_group("buffer_pool");
    
    group.bench_function("acquire_small", |b| {
        b.iter(|| {
            let buf = pool.acquire(BufferSize::Small);
            black_box(buf);
        })
    });

    group.bench_function("acquire_release_cycle", |b| {
        b.iter(|| {
            let buf = pool.acquire(BufferSize::Medium).unwrap();
            black_box(&buf);
            drop(buf);
        })
    });

    group.finish();
}

fn connection_slab_benchmark(c: &mut Criterion) {
    use mytunnel_server::pool::ConnectionSlab;

    let slab: ConnectionSlab<u64> = ConnectionSlab::new(10000);

    let mut group = c.benchmark_group("connection_slab");
    
    group.bench_function("insert_remove", |b| {
        b.iter(|| {
            let handle = slab.insert(42).unwrap();
            black_box(slab.get(handle));
            slab.remove(handle);
        })
    });

    group.finish();
}

fn metrics_benchmark(c: &mut Criterion) {
    use mytunnel_server::metrics::METRICS;

    let mut group = c.benchmark_group("metrics");
    group.throughput(Throughput::Elements(1));

    group.bench_function("counter_increment", |b| {
        b.iter(|| {
            METRICS.bytes_rx(black_box(1024));
        })
    });

    group.bench_function("snapshot", |b| {
        b.iter(|| {
            let snapshot = METRICS.snapshot();
            black_box(snapshot);
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    buffer_pool_benchmark,
    connection_slab_benchmark,
    metrics_benchmark,
);
criterion_main!(benches);

