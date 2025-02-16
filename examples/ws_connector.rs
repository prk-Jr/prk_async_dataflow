use prk_async_dataflow::{ DataConnector, WebSocketConnector};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ws_url = "ws://echo.websocket.events/";
    let connector = WebSocketConnector::new(ws_url)?;
    let data = connector.fetch().await?;
    println!("Received data:\n{}", String::from_utf8_lossy(&data));
    Ok(())
}