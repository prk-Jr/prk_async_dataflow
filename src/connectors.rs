use async_trait::async_trait;
use reqwest::{Client, Url};
use tokio_tungstenite::{connect_async, tungstenite::Error as WsError};
use futures::StreamExt;

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] WsError),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}



#[async_trait]
pub trait DataConnector {
    async fn fetch(&self) -> Result<Vec<u8>, ConnectorError>;
    async fn stream(&self) -> Result<impl StreamExt<Item = Result<Vec<u8>, ConnectorError>>, ConnectorError>;
}

pub struct HttpConnector {
    client: Client,
    url: Url,
}

impl HttpConnector {
    pub fn new(url: &str) -> Result<Self, ConnectorError> {
        Ok(Self {
            client: Client::new(),
            url: Url::parse(url).map_err(|e| ConnectorError::InvalidUrl(e.to_string()))?,
        })
    }
}

#[async_trait]
impl DataConnector for HttpConnector {
    async fn fetch(&self) -> Result<Vec<u8>, ConnectorError> {
        let response = self.client.get(self.url.as_ref()).send().await?;
        let data = response.bytes().await?;
        Ok(data.to_vec())
    }

    async fn stream(&self) -> Result<impl StreamExt<Item = Result<Vec<u8>, ConnectorError>>, ConnectorError> {
        let response = self.client.get(self.url.as_ref()).send().await?;
        Ok(response.bytes_stream()
            .map(|chunk| chunk.map(|b| b.to_vec()).map_err(Into::into)))
    }
}

pub struct WebSocketConnector {
    url: Url,
}

impl WebSocketConnector {
    pub fn new(url: &str) -> Result<Self, ConnectorError> {
        Ok(Self {
            url: Url::parse(url).map_err(|e| ConnectorError::InvalidUrl(e.to_string()))?,
        })
    }
}

#[async_trait]
impl DataConnector for WebSocketConnector {
    async fn fetch(&self) -> Result<Vec<u8>, ConnectorError> {
        let (mut ws_stream, _) = connect_async(&self.url).await?;
        let mut data = Vec::new();
        
        while let Some(msg) = ws_stream.next().await {
            let msg = msg?;
            data.extend_from_slice(msg.into_data().as_slice());
        }
        
        Ok(data)
    }

    async fn stream(&self) -> Result<impl StreamExt<Item = Result<Vec<u8>, ConnectorError>>, ConnectorError> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        Ok(ws_stream
            .map(|msg| msg.map(|m| m.into_data()).map_err(Into::into)))
    }
}