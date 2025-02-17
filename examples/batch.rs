use prk_async_dataflow::{AsyncJsonParser, DataConnector, HttpConnector, ParserConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct Post {
    id: i64,
    title: String,
    body: String,
}

#[tokio::main]
async fn main() {
    let connector = HttpConnector::new("https://jsonplaceholder.typicode.com/posts").unwrap();
    let data = connector.fetch().await.unwrap();
    
    let mut parser = AsyncJsonParser::with_config(
        std::io::Cursor::new(data.clone()),
        ParserConfig {
            batch_size: 5,
            ..Default::default()
        },
    );

    while let Ok(batch) = parser.next_batch::<Post>().await {
        if batch.is_empty() {
            break;
        }
        println!("Batch: {:?}", batch);
    }

    let mut parser = AsyncJsonParser::new(
        std::io::Cursor::new(data),
       
    );
    while let Ok(batch) = parser.next::<Post>().await {
        println!("Individual: {:?}", batch);
    }
}