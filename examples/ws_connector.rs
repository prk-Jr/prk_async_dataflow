use prk_async_dataflow::{AsyncJsonParser, DataConnector,  StreamToAsyncRead, WebSocketConnector};
use tokio_stream::StreamExt;
use std::io;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connector = WebSocketConnector::new("ws://echo.websocket.org", None).unwrap();
    let stream = connector.stream().await.map_err(|f| format!("WSERROR: {:?}", f.to_string()))?;

    // Adapt the stream into an AsyncRead
    let stream = stream.map(|result| {
        result.map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    });
    let async_read = StreamToAsyncRead::new(stream);

    // Create the parser
    let mut parser = AsyncJsonParser::new(
        async_read,
    );
    println!("Received data:\n{:?}",parser.next::<String>().await);
    Ok(())
}