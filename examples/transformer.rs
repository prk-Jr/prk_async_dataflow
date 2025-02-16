use std::io::Cursor;
use prk_async_dataflow::{AsyncJsonParser, DataConnector, FeatureTransformer, HttpConnector};
use serde::{Deserialize, Serialize};
use simd_json::{base::ValueAsScalar, borrowed::Value};
use tokio_stream::StreamExt;

#[derive(Debug, Deserialize, Serialize)]
struct Post {
    id: i64,
    title: String,
    body: String,
}

#[tokio::main]
async fn main() {
    let connector = HttpConnector::new("https://jsonplaceholder.typicode.com/posts");
    let connector = connector.unwrap();
    let data = connector.fetch().await.unwrap();
    let reader = Cursor::new(data);
    let parser = AsyncJsonParser::new(reader);

    let mut transformer = FeatureTransformer::new();
    transformer.add_mapping("title".to_string(), Box::new(|v| {
        // Transform the title to uppercase
        if let Some(title) = v.as_str() {
            Value::String(title.to_uppercase().into())
        } else {
            v // Return the original value if it's not a string
        }
    }));

    // Parse the array of posts
    let mut stream = parser.into_stream::<Vec<Post>>();
    while let Some(result) = stream.next().await {
        match result {
            Ok(posts) => {
                for mut post in posts {
                    // Apply the transformation to the title
                    post.title = post.title.to_uppercase();
                    println!("Transformed Post: {:#?}", post);
                }
            }
            Err(e) => {
                eprintln!("Error parsing JSON: {}", e);
            }
        }
    }
}