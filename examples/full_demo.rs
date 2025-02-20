use futures::StreamExt;
use prk_async_dataflow::{
    connectors::{AuthMethod, DataConnector, HttpConfig, HttpConnector, WebSocketConnector},
    AsyncJsonParser, FeatureTransformer, ParserConfig, StreamToAsyncRead
};
use serde::{Deserialize, Serialize};
use simd_json::{base::ValueAsScalar, OwnedValue};
use std::{sync::Arc, time::Duration};
#[derive(Debug, Deserialize, Serialize)]
struct SensorData {
    sensor_id: String,
    temperature: f64,
    timestamp: u64,
}

#[tokio::main]
async fn main() {
    // Example 1: HTTP Connector with Transformer
    let http_config = HttpConfig {
        auth: Some(AuthMethod::ApiKey {
            key: "secret-key".into(),
            header: "X-API-KEY".into(),
        }),
        ..Default::default()
    };
    
    let mut transformer = FeatureTransformer::new();
    transformer.add_mapping("temperature".into(), Box::new(|v| {
        if let Some(n) = v.as_f64() {
            OwnedValue::from(n * 9.0/5.0 + 32.0) // Convert C to F
        } else {
            v
        }
    }));

    let http_conn = HttpConnector::new(
        "http://sensor-api.com/data?format=ndjson",
        http_config
    ).unwrap();

    let http_stream = http_conn.stream().await.unwrap();
    let http_reader = StreamToAsyncRead::new(http_stream);

    let parser_config = ParserConfig {
        transformer: Some(Arc::new(transformer)),
        ..Default::default()
    };

    let mut parser = AsyncJsonParser::with_config(http_reader, parser_config);
    while let Ok(batch) = parser.next_batch::<SensorData>().await {
        println!("Processed batch: {:?}", batch);
    }

    // Example 2: WebSocket Connector with Error Handling
    let ws_conn = WebSocketConnector::new(
        "ws://chat-service.com/feed",
        Some(AuthMethod::BearerToken("ws-token".into()))
    ).unwrap();

        let ws_stream = ws_conn.stream().await.unwrap();
        let ws_reader = StreamToAsyncRead::new(ws_stream);

        let mut ws_parser = AsyncJsonParser::with_config(ws_reader, ParserConfig {
            skip_invalid: true,
            timeout: Some(Duration::from_secs(5)),
            ..Default::default()
        });

        while let Ok(msg) = ws_parser.next::<String>().await {
            println!("Real-time message: {:?}", msg);
        }

    // Example 3: Local File Processing with Metrics
    let file_data = r#"
        {"sensor_id": "A1", "temperature": 23.5, "timestamp": 1620000000}
        {"sensor_id": "B2", "temperature": "invalid", "timestamp": 1620000001}
        {"sensor_id": "C3", "temperature": 19.8, "timestamp": 1620000002}
    "#.as_bytes();
    
    let file_parser = AsyncJsonParser::with_config(
        file_data,
        ParserConfig {
            skip_invalid: true,
            ..Default::default()
        }
    );

    let mut stream = file_parser.into_stream::<SensorData>();
    while let Some(result) = stream.next().await {
        match result {
            Ok(data) => println!("Valid data: {:?}", data),
            Err(e) => eprintln!("Error processing data: {}", e),
        }
    }

    // Print metrics
    #[cfg(feature = "metrics")]
    {
        println!("Metrics:\n{}", prk_async_dataflow::metrics::gather_metrics());
    }
}
