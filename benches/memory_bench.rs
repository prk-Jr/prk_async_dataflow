use criterion::{criterion_group, criterion_main, Criterion, Throughput, BenchmarkId};
use prk_async_dataflow::*;
use serde::Deserialize;
use tokio::runtime::Runtime;
use std::alloc::System;
use std::sync::atomic::{AtomicUsize, Ordering};

#[global_allocator]
static ALLOC: TrackingAllocator<System> = TrackingAllocator::new(System);

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

struct TrackingAllocator<A>(A);

impl<A> TrackingAllocator<A> {
    const fn new(alloc: A) -> Self {
        Self(alloc)
    }
}

unsafe impl<A: std::alloc::GlobalAlloc> std::alloc::GlobalAlloc for TrackingAllocator<A> {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let ptr = self.0.alloc(layout);
        if !ptr.is_null() {
            ALLOCATED.fetch_add(layout.size(), Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        self.0.dealloc(ptr, layout);
        ALLOCATED.fetch_sub(layout.size(), Ordering::SeqCst);
    }
}

fn allocated() -> usize {
    ALLOCATED.load(Ordering::SeqCst)
}

#[derive(Deserialize)]
struct SampleData {
    id: u64,
    value: String,
}

fn create_large_json(count: usize) -> Vec<u8> {
    let mut data = Vec::new();
    for i in 0..count {
        data.extend(format!(r#"{{"id":{},"value":"Value {}"}}\n"#, i, i).as_bytes());
    }
    data
}

async fn parse_data(data: &[u8], count: usize) {
    let reader = tokio::io::BufReader::new(data);
    let mut parser = AsyncJsonParser::new(reader);
    let mut parsed = 0;
    
    loop {
        match parser.next::<SampleData>().await {
            Ok(_) => parsed += 1,
            Err(JsonParserError::IncompleteData) => break,
            _ => panic!("Unexpected error"),
        }
    }
    
    assert_eq!(parsed, count);
}

fn memory_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("memory_usage");
    group.sample_size(10);
    
    for count in [100, 1000, 10_000].iter() {
        let data = create_large_json(*count);
        group.throughput(Throughput::Bytes(data.len() as u64));
        
        group.bench_with_input(
            BenchmarkId::from_parameter(count), 
            &data,
            |b, data| {
                b.iter(|| {
                    let start_mem = allocated();
                    rt.block_on(async {
                        parse_data(data, *count).await
                    });
                    let end_mem = allocated();
                    println!("Memory delta for {} items: {} bytes", count, end_mem - start_mem);
                });
            }
        );
    }
    
    group.finish();
}

criterion_group!(benches, memory_benchmark);
criterion_main!(benches);