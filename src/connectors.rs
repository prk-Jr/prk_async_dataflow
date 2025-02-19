use async_trait::async_trait;
use reqwest::{Client, Url, Method, header::{HeaderMap,  AUTHORIZATION}};
use tokio_tungstenite::{connect_async, tungstenite::handshake::client::Request, tungstenite::Error as WsError};
use futures::{StreamExt, TryStreamExt};
use std::time::Duration;

#[async_trait]
pub trait DataConnector {
    async fn fetch(&self) -> Result<Vec<u8>, ConnectorError>;
    async fn stream(&self) -> Result<impl StreamExt<Item = Result<Vec<u8>, ConnectorError>>, ConnectorError>;
}
// connectors.rs


#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] WsError),
    #[error("Invalid configuration: {0}")]
    Config(String),
    #[error("Authentication failed: {0}")]
    Auth(String),
}


#[derive(Clone, Debug)]
pub enum AuthMethod {
    BearerToken(String),
    ApiKey { key: String, header: String },
    OAuth2 { token: String, token_type: String },
    Basic { username: String, password: String },
}

#[derive(Clone, Debug)]
pub struct HttpConfig {
   pub method: Method,
   pub headers: HeaderMap,
   pub body: Option<Vec<u8>>,
   pub timeout: Duration,
   pub auth: Option<AuthMethod>,
   pub retries: u32,
   pub query_params: Vec<(String, String)>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            method: Method::GET,
            headers: HeaderMap::new(),
            body: None,
            timeout: Duration::from_secs(30),
            auth: None,
            retries: 3,
            query_params: Vec::new(),
        }
    }
}

pub struct HttpConnector {
    client: Client,
    url: Url,
    config: HttpConfig,
}

impl HttpConnector {
    pub fn new(url: &str, config: HttpConfig) -> Result<Self, ConnectorError> {
        let mut url = Url::parse(url).map_err(|e| ConnectorError::Config(e.to_string()))?;
        
        // Add query parameters
        let query = config.query_params.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");
        if !query.is_empty() {
            url.set_query(Some(&query));
        }

        Ok(Self {
            client: Client::new(),
            url,
            config,
        })
    }

    fn apply_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.config.auth {
            Some(AuthMethod::BearerToken(token)) => request.bearer_auth(token),
            Some(AuthMethod::ApiKey { key, header }) => request.header(header, key),
            Some(AuthMethod::OAuth2 { token, token_type }) => request.header(AUTHORIZATION, format!("{} {}", token_type, token)),
            Some(AuthMethod::Basic { username, password }) => request.basic_auth(username, Some(password)),
            None => request,
        }
    }
}

#[async_trait]
impl DataConnector for HttpConnector {
    async fn fetch(&self) -> Result<Vec<u8>, ConnectorError> {
        let mut request = self.client.request(self.config.method.clone(), self.url.as_ref())
            .timeout(self.config.timeout);

        request = self.apply_auth(request);
        
        if let Some(body) = &self.config.body {
            request = request.body(body.clone());
        }

        for _ in 0..self.config.retries {
            let response = request.try_clone()
                .ok_or_else(|| ConnectorError::Config("Request cannot be cloned".into()))?
                .send()
                .await?;

            if response.status().is_success() {
                let data = response.bytes().await?;
                return Ok(data.to_vec());
            }
        }

        Err(ConnectorError::Auth("Max retries exceeded".into()))
    }

    async fn stream(&self) -> Result<impl StreamExt<Item = Result<Vec<u8>, ConnectorError>>, ConnectorError> {
        let mut request = self.client.request(self.config.method.clone(), self.url.as_ref())
            .timeout(self.config.timeout);

        request = self.apply_auth(request);
        
        if let Some(body) = &self.config.body {
            request = request.body(body.clone());
        }

        let response = request.send().await?;
        Ok(response.bytes_stream()
            .map_ok(|b| b.to_vec())
            .map_err(Into::into))
    }
}

pub struct WebSocketConnector {
    url: Url,
    headers: HeaderMap,
    auth: Option<AuthMethod>,
}

impl WebSocketConnector {
    pub fn new(url: &str, auth: Option<AuthMethod>) -> Result<Self, ConnectorError> {
        Ok(Self {
            url: Url::parse(url).map_err(|e| ConnectorError::Config(e.to_string()))?,
            headers: HeaderMap::new(),
            auth,
        })
    }

    fn build_request(&self) -> Result<Request, ConnectorError> {
        let mut request = Request::builder().uri(self.url.as_str());
        
        if let Some(auth) = &self.auth {
            match auth {
                AuthMethod::BearerToken(token) => {
                    request = request.header(AUTHORIZATION, format!("Bearer {}", token));
                }
                AuthMethod::ApiKey { key, header } => {
                    request = request.header(header, key);
                }
                _ => return Err(ConnectorError::Auth("Unsupported auth method for WebSocket".into())),
            }
        }

        Ok(request.body(()).map_err(|e| ConnectorError::Config(e.to_string()))?)
    }
}

#[async_trait]
impl DataConnector for WebSocketConnector {
    async fn fetch(&self) -> Result<Vec<u8>, ConnectorError> {
        let request = self.build_request()?;
        let (mut ws_stream, _) = connect_async(request).await?;
        let mut data = Vec::new();
        
        while let Some(msg) = ws_stream.next().await {
            let msg = msg?;
            data.extend_from_slice(msg.into_data().as_slice());
        }
        
        Ok(data)
    }

    async fn stream(&self) -> Result<impl StreamExt<Item = Result<Vec<u8>, ConnectorError>>, ConnectorError> {
        let request = self.build_request()?;
        let (ws_stream, _) = connect_async(request).await?;
        Ok(ws_stream
            .map(|msg| msg.map(|m| m.into_data()).map_err(Into::into)))
    }
}

impl From<ConnectorError> for std::io::Error {
    fn from(err: ConnectorError) -> Self {
        use ConnectorError::*;
        match err {
            Http(e) => std::io::Error::new(std::io::ErrorKind::Other, e),
            WebSocket(e) => std::io::Error::new(std::io::ErrorKind::Other, e),
            Config(msg) => std::io::Error::new(std::io::ErrorKind::InvalidInput, msg),
            Auth(msg) => std::io::Error::new(std::io::ErrorKind::PermissionDenied, msg),
        }
    }
}