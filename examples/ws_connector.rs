use futures::StreamExt;
use reqwest::Url;
use tokio_tungstenite::connect_async;
use prk_async_dataflow::DataConnector;

pub struct WebSocketConnector {
    url: Url,
}

impl WebSocketConnector {
    pub fn new(url: Url) -> Self {
        Self { url }
    }
}

#[async_trait::async_trait]
impl DataConnector for WebSocketConnector {
    async fn fetch(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        let (_, mut read) = ws_stream.split();
        let mut data = Vec::new();
        while let Some(message) = read.next().await {
            match message {
                Ok(msg) => match msg {
                    tokio_tungstenite::tungstenite::Message::Text(txt) => {
                        data.extend_from_slice(txt.as_bytes())
                    }
                    tokio_tungstenite::tungstenite::Message::Binary(bin) => {
                        data.extend_from_slice(&bin)
                    }
                    tokio_tungstenite::tungstenite::Message::Close(_) => break,
                    _ => {},
                },
                Err(e) => {
                    // Check if the error is a reset without closing handshake and treat it as EOF.
                    if let tokio_tungstenite::tungstenite::Error::Protocol(
                        tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                    ) = e
                    {
                        break;
                    } else {
                        return Err(Box::new(e));
                    }
                }
            }
        }
        Ok(data)
    }
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ws_url = reqwest::Url::parse("ws://echo.websocket.events/")?;
    let connector = WebSocketConnector::new(ws_url);
    let data = connector.fetch().await?;
    println!("Received data:\n{}", String::from_utf8_lossy(&data));
    Ok(())
}
