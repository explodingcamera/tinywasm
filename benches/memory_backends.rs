use criterion::measurement::WallTime;
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use tinywasm::{LinearMemory, PagedMemory, VecMemory};

const PAGE_SIZE: usize = 64 * 1024;
const CHUNK_SIZE: usize = 4 * 1024;
const GROW_STEPS: usize = 32;
const BENCH_MEASUREMENT_TIME: std::time::Duration = std::time::Duration::from_secs(10);

const MEMORY_LEN: usize = PAGE_SIZE * 4;
const CONTIGUOUS_OFFSET: usize = 1024;
const CONTIGUOUS_LEN: usize = 2048;
const CROSS_CHUNK_OFFSET: usize = CHUNK_SIZE - 512;
const CROSS_CHUNK_LEN: usize = CHUNK_SIZE * 2;

fn bench_grow<M, F>(group: &mut BenchmarkGroup<'_, WallTime>, backend: &str, make_memory: F)
where
    M: LinearMemory,
    F: Fn() -> M + Copy,
{
    group.bench_function(BenchmarkId::new("grow", backend), |b| {
        b.iter_batched(
            make_memory,
            |mut memory| {
                for page_count in 2..=GROW_STEPS + 1 {
                    memory.grow_to(page_count * PAGE_SIZE).unwrap();
                }
                black_box(memory.len())
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_write_all<M: LinearMemory>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    backend: &str,
    workload: &str,
    mut memory: M,
    offset: usize,
    len: usize,
) {
    let src = vec![0xA5; len];
    group.bench_function(BenchmarkId::new(format!("write_all/{workload}"), backend), |b| {
        b.iter(|| {
            memory.write_all(offset, black_box(&src)).unwrap();
            black_box(memory.len())
        })
    });
}

fn bench_read_exact<M: LinearMemory>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    backend: &str,
    workload: &str,
    mut memory: M,
    offset: usize,
    len: usize,
) {
    let src = vec![0x5A; len];
    memory.write_all(offset, &src).unwrap();

    let mut dst = vec![0; len];
    group.bench_function(BenchmarkId::new(format!("read_exact/{workload}"), backend), |b| {
        b.iter(|| {
            memory.read_exact(offset, black_box(&mut dst)).unwrap();
            black_box(&dst);
        })
    });
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_backends");
    group.measurement_time(BENCH_MEASUREMENT_TIME);

    bench_grow(&mut group, "vec", || VecMemory::try_new(PAGE_SIZE).expect("bench memory should be constructible"));
    bench_grow(&mut group, "paged", || {
        PagedMemory::try_new(PAGE_SIZE, CHUNK_SIZE).expect("bench memory should be constructible")
    });

    bench_write_all(
        &mut group,
        "vec",
        "contiguous",
        VecMemory::try_new(MEMORY_LEN).expect("bench memory should be constructible"),
        CONTIGUOUS_OFFSET,
        CONTIGUOUS_LEN,
    );
    bench_write_all(
        &mut group,
        "paged",
        "contiguous",
        PagedMemory::try_new(MEMORY_LEN, CHUNK_SIZE).expect("bench memory should be constructible"),
        CONTIGUOUS_OFFSET,
        CONTIGUOUS_LEN,
    );
    bench_read_exact(
        &mut group,
        "vec",
        "contiguous",
        VecMemory::try_new(MEMORY_LEN).expect("bench memory should be constructible"),
        CONTIGUOUS_OFFSET,
        CONTIGUOUS_LEN,
    );
    bench_read_exact(
        &mut group,
        "paged",
        "contiguous",
        PagedMemory::try_new(MEMORY_LEN, CHUNK_SIZE).expect("bench memory should be constructible"),
        CONTIGUOUS_OFFSET,
        CONTIGUOUS_LEN,
    );

    bench_write_all(
        &mut group,
        "vec",
        "cross_chunk",
        VecMemory::try_new(MEMORY_LEN).expect("bench memory should be constructible"),
        CROSS_CHUNK_OFFSET,
        CROSS_CHUNK_LEN,
    );
    bench_write_all(
        &mut group,
        "paged",
        "cross_chunk",
        PagedMemory::try_new(MEMORY_LEN, CHUNK_SIZE).expect("bench memory should be constructible"),
        CROSS_CHUNK_OFFSET,
        CROSS_CHUNK_LEN,
    );
    bench_read_exact(
        &mut group,
        "vec",
        "cross_chunk",
        VecMemory::try_new(MEMORY_LEN).expect("bench memory should be constructible"),
        CROSS_CHUNK_OFFSET,
        CROSS_CHUNK_LEN,
    );
    bench_read_exact(
        &mut group,
        "paged",
        "cross_chunk",
        PagedMemory::try_new(MEMORY_LEN, CHUNK_SIZE).expect("bench memory should be constructible"),
        CROSS_CHUNK_OFFSET,
        CROSS_CHUNK_LEN,
    );

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
