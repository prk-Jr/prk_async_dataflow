use criterion::{criterion_group, criterion_main, Criterion, Throughput, BenchmarkId};
use prk_async_dataflow::*;
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;

#[derive(Deserialize, Serialize)]
struct SampleData {
    id: u64,
    value: String,
}

fn create_large_json(count: usize) -> Vec<u8> {
    let mut data = Vec::new();
    for i in 0..count {
        data.extend(format!(r#"{{"id":{},"value":"Value {}"}}"#, i, i).as_bytes());
        data.push(b'\n');
    }
    data
}

async fn parse_data(data: &[u8], expected_count: usize) {
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
    
    assert_eq!(parsed, expected_count, "Parsed {} entries, expected {}", parsed, expected_count);
}

async fn concurrent_parse(data: &[u8], total_count: usize, concurrency: usize) {
    // Split the dataset by newline.
    let lines: Vec<&[u8]> = data.split(|&b| b == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), total_count);

    // Partition the lines into equal chunks.
    let chunk_size = total_count / concurrency;
    
    let tasks = (0..concurrency).map(|i| {
        // Combine the lines for this partition back into one byte vector.
        let chunk = lines[i * chunk_size..(i + 1) * chunk_size]
            .join(&b'\n');
        tokio::spawn(async move {
            parse_data(&chunk, chunk_size).await
        })
    }).collect::<Vec<_>>();
    
    for task in tasks {
        task.await.unwrap();
    }
}

fn concurrency_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("concurrency");
    group.sample_size(10);
    
    let count = 10_000;
    let data = create_large_json(count);
    
    for &concurrency in &[1, 2, 4, 8] {
        group.throughput(Throughput::Bytes((data.len() / concurrency) as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrency), 
            &concurrency,
            |b, &concurrency| {
                b.iter(|| {
                    rt.block_on(async {
                        concurrent_parse(&data, count, concurrency).await
                    });
                });
            }
        );
    }
    
    group.finish();
}

criterion_group!(benches, concurrency_benchmark);
criterion_main!(benches);
