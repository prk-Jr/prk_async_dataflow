use criterion::{criterion_group, criterion_main, Criterion};
use prk_async_dataflow::*;
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;

#[derive(Deserialize, Serialize)]
struct SampleData {
    id: u64,
    value: String,
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

fn profile_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let data = include_bytes!("./test-data/large-dataset.txt");
    let count = 10_199; // Update with actual count in your dataset
    
    c.bench_function("profile", |b| {
        b.iter(|| {
            rt.block_on(async {
                parse_data(data, count).await
            });
        });
    });
}

criterion_group!(benches, profile_benchmark);
criterion_main!(benches);