use prk_async_dataflow::*;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Debug, Deserialize, Serialize)]
struct ThoughtChunk {
    step: u32,
    thought: String,
    action: String,
}

async fn simulate_llm_stream() -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel(10);
    
    tokio::spawn(async move {
        let chunks = vec![
            r#"{"step":1,"thought":"Initializing","#,
            r#""action":"Start process"}"#,
            "\n",
            r#"{"step":2,"thought":"Analyzing input","action":"Fetch data"}"#,
            "\n",
            r#"{"step":3,"thought":"Processing","action":"Transform"}"#,
            "\n",
        ];
        
        for chunk in chunks {
            if tx.send(chunk.as_bytes().to_vec()).await.is_err() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    });
    
    rx
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rx = simulate_llm_stream().await;
    let reader = ChannelReader::new(rx);
    let mut parser = AsyncJsonParser::with_config(
        reader,
        ParserConfig {
            skip_invalid: true,
            ..Default::default()
        }
    );

    loop {
        match parser.next::<ThoughtChunk>().await {
            Ok(thought) => {
                println!("[Step {}] {}", thought.step, thought.thought);
                println!("Action: {}\n", thought.action);
            }
            Err(JsonParserError::IncompleteData) => break,
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }

    Ok(())
}