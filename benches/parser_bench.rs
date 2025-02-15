use criterion::{criterion_group, criterion_main, Criterion, Throughput, BenchmarkId};
use prk_async_dataflow::*;
use serde::Deserialize;
use tokio::runtime::Runtime;

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

fn json_parsing_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("json_parsing");
    group.sample_size(10);
    
    for count in [100, 1000, 10_000].iter() {
        let data = create_large_json(*count);
        group.throughput(Throughput::Bytes(data.len() as u64));
        
        group.bench_with_input(
            BenchmarkId::from_parameter(count), 
            &data,
            |b, data| {
                b.iter(|| {
                    rt.block_on(async {
                        parse_data(data, *count).await
                    });
                });
            }
        );
    }
    
    group.finish();
}

criterion_group!(benches, json_parsing_benchmark);
criterion_main!(benches);