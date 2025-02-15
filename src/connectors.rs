use async_trait::async_trait;
use futures::StreamExt;
use reqwest::{Client, Url};
use tokio_tungstenite::connect_async;

#[async_trait]
pub trait DataConnector {
    async fn fetch(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
}

pub struct HttpConnector {
    url: String,
    client: Client,
}

impl HttpConnector {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl DataConnector for HttpConnector {
    async fn fetch(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let response = self.client.get(&self.url).send().await?;
        let data = response.bytes().await?;
        Ok(data.to_vec())
    }
}

pub struct WebSocketConnector {
    url: Url,
}

impl WebSocketConnector {
    pub fn new(url: Url) -> Self {
        Self { url }
    }
}

#[async_trait]
impl DataConnector for WebSocketConnector {
    async fn fetch(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        let (_, mut read) = ws_stream.split();
        let mut data = Vec::new();
        while let Some(message) = read.next().await {
            data.extend_from_slice(&message?.into_data());
        }
        Ok(data)
    }
}