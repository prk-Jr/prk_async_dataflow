use std::io::Cursor;
use prk_async_dataflow::{AsyncJsonParser, DataConnector, FeatureTransformer, HttpConnector};
use serde::{Deserialize, Serialize};
use simd_json::{base::{ValueAsContainer, ValueAsScalar}, OwnedValue};
use tokio_stream::StreamExt;

#[derive(Debug, Deserialize, Serialize)]
struct Post {
    id: i64,
    title: String,
    body: String,
}

#[tokio::main]
async fn main() {
    let connector = HttpConnector::new("https://jsonplaceholder.typicode.com/posts".to_string());
    let data = connector.fetch().await.unwrap();
    let reader = Cursor::new(data);
    let parser = AsyncJsonParser::new(reader);

    let mut transformer = FeatureTransformer::new();
    transformer.add_mapping("title".to_string(), Box::new(|v| {
        // Transform the title to uppercase
        if let Some(title) = v.as_str() {
            OwnedValue::String(title.to_uppercase().into()).into()
        } else {
            v.clone() // Return the original value if it's not a string
        }
    }));

    // Parse the array of posts
    let mut stream = parser.into_stream::<OwnedValue>();
    while let Some(result) = stream.next().await {
        match result {
            Ok(value) => {
                if let Some(posts) = value.as_array() {
                    for post in posts {
                        // Convert the `OwnedValue` to a JSON string and then deserialize it
                        let mut json_string = post.to_string();
                        unsafe {
                            if let Ok(mut post) = simd_json::serde::from_str::<Post>(&mut json_string) {
                                // Apply the transformation to the title
                                post.title = post.title.to_uppercase();
                                println!("Transformed Post: {:#?}", post);
                            }
                        }
                            // Apply the transformation to the title
                            println!("Transformed Post: {:#?}", post);
                        }
                    
                } else {
                    // Convert the `OwnedValue` to a JSON string and then deserialize it
                    let mut json_string = value.to_string();
                    unsafe  {
                        if let Ok(mut post) = simd_json::serde::from_str::<Post>(&mut json_string) {
                            // Apply the transformation to the title
                            post.title = post.title.to_uppercase();
                            println!("Transformed Post: {:#?}", post);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error parsing JSON: {}", e);
            }
        }
    }
}