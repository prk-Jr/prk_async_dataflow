use std::io::Cursor;
use prk_async_dataflow::{AsyncJsonParser, DataConnector, FeatureTransformer, HttpConnector};
use serde::{Deserialize, Serialize};
use simd_json::base::ValueAsScalar;
use tokio_stream::StreamExt;

#[derive(Debug, Deserialize, Serialize)]
struct Post {
    id: i64,
    title: String,
    body: String,
}
// transformer.rs
#[tokio::main]
async fn main() {
    let connector = HttpConnector::new("https://jsonplaceholder.typicode.com/posts").unwrap();
    let data = connector.fetch().await.unwrap();
    let reader = Cursor::new(data);
    

    let mut transformer = FeatureTransformer::new();
    transformer.add_mapping("title".to_string(), Box::new(|mut v| {
        // Transform the title to uppercase
        v = v.as_str().unwrap().to_uppercase().into();
        v
    }));

    let parser = AsyncJsonParser::new(reader, );

    let mut stream = parser.into_stream::<simd_json::OwnedValue>();
    while let Some(result) = stream.next().await {
        match result {
            Ok(mut value) => {
                value = transformer.transform(value.into()).into();
                match simd_json::serde::from_owned_value::<Post>(value) {
                    Ok(post) => println!("Transformed Post: {:#?}", post),
                    Err(e) => eprintln!("Deserialization error: {}", e),
                }
            }
            Err(e) => eprintln!("Error parsing JSON: {}", e),
        }
    }
}