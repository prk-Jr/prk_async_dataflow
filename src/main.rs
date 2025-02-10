use serde::de::DeserializeOwned;
use serde_json::{self, error::Category};
use std::io::{Error as IoError, ErrorKind};
use tokio::io::{AsyncRead, AsyncReadExt, Error, ReadBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

/// A streaming JSON parser that buffers data until it can parse a complete JSON value.
struct AsyncJsonParser<R> {
    reader: R,
    buffer: Vec<u8>,
}

impl<R: AsyncRead + Unpin> AsyncJsonParser<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Vec::new(),
        }
    }

    /// Repeatedly reads data into an internal buffer and tries to deserialize a JSON value.
    /// If the data is incomplete, it continues to read from the stream.
    pub async fn next<T: DeserializeOwned>(&mut self) -> Result<T, Error> {
        let mut chunk = [0u8; 1024];

        loop {
            match serde_json::from_slice::<T>(&self.buffer) {
                Ok(value) => {
                    // On a successful parse, clear the buffer and return the value.
                    self.buffer.clear();
                    return Ok(value);
                }
                // If the error indicates incomplete data, try reading more.
                Err(e) if e.classify() == Category::Eof => {
                    let bytes_read = self.reader.read(&mut chunk).await?;
                    if bytes_read == 0 {
                        return Err(Error::new(
                            ErrorKind::UnexpectedEof,
                            "Incomplete JSON data",
                        ));
                    }
                    self.buffer.extend_from_slice(&chunk[..bytes_read]);
                }
                // For any other error, return it.
                Err(e) => return Err(Error::new(ErrorKind::InvalidData, e)),
            }
        }
    }
}

/// A custom asynchronous reader that receives data in chunks from a Tokio mpsc channel.
/// This is used to simulate data arriving gradually (i.e. streaming).
struct ChannelReader {
    rx: mpsc::Receiver<Vec<u8>>,
    buffer: Vec<u8>,
}

impl ChannelReader {
    pub fn new(rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            rx,
            buffer: Vec::new(),
        }
    }
}

impl AsyncRead for ChannelReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), IoError>> {
        // If our internal buffer is empty, try to receive a new chunk.
        if self.buffer.is_empty() {
            match Pin::new(&mut self.rx).poll_recv(cx) {
                Poll::Ready(Some(chunk)) => {
                    self.buffer = chunk;
                }
                // If the channel is closed, signal EOF.
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                // No data available yet.
                Poll::Pending => return Poll::Pending,
            }
        }
        // Copy as many bytes as possible into the provided buffer.
        let remaining = buf.remaining();
        let n = std::cmp::min(remaining, self.buffer.len());
        buf.put_slice(&self.buffer[..n]);
        self.buffer.drain(..n);
        Poll::Ready(Ok(()))
    }
}

/// Main function to experience streaming JSON with a delayed stream.
/// A background task sends the JSON data in two parts (with a delay), and the parser waits
/// for the complete JSON to arrive before parsing it into a `Person` struct.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Define the data structure for our JSON.
    #[derive(Debug, serde::Deserialize)]
    struct Person {
        name: String,
        age: u32,
    }

    // Create a channel to simulate a streaming source.
    let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
    let reader = ChannelReader::new(rx);
    let mut parser = AsyncJsonParser::new(reader);

    // Spawn a background task that sends the JSON data in two parts.
    tokio::spawn(async move {
        // Send the first half of the JSON.
        tx.send(b"{ \"name\": \"A".to_vec()).await.unwrap();
        // Wait 100 milliseconds to simulate network delay.
        sleep(Duration::from_millis(100)).await;
        // Send the remaining part of the JSON.
        tx.send(b"lice\", \"age\": 30 }".to_vec()).await.unwrap();
    });

    println!("Waiting for JSON data to arrive in a streaming manner...");

    // Block until the parser successfully reads and parses the complete JSON.
    let person: Person = parser.next().await.unwrap();
    println!("Parsed person: {:?}", person);

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;
    use std::io::Cursor;
    use tokio::sync::mpsc;
    use tokio::time::sleep;

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Person {
        name: String,
        age: u32,
    }

    /// Parse JSON provided in a single complete chunk.
    #[tokio::test]
    async fn test_async_json_parser_single_chunk() {
        let data = b"{ \"name\": \"Alice\", \"age\": 30 }";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(
            person,
            Person {
                name: "Alice".to_string(),
                age: 30
            }
        );
    }

    /// Parse JSON split across multiple chunks that are all immediately available.
    #[tokio::test]
    async fn test_async_json_parser_multiple_chunks() {
        let chunks: Vec<&[u8]> = vec![
            b"{ \"name\": \"A",
            b"lice\", \"age\":",
            b" 30 }",
        ];
        let stream = BufReader::new(Cursor::new(
            chunks.into_iter().flat_map(|s| s.to_vec()).collect::<Vec<u8>>(),
        ));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(
            person,
            Person {
                name: "Alice".to_string(),
                age: 30
            }
        );
    }

    /// Return an UnexpectedEof error when the JSON data is incomplete.
    #[tokio::test]
    async fn test_async_json_parser_incomplete_data() {
        let data = b"{ \"name\": \"Alice\", \"age\": ";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::UnexpectedEof);
    }

    /// Return an InvalidData error when the JSON data is invalid.
    #[tokio::test]
    async fn test_async_json_parser_invalid_data() {
        let data = b"{ \"name\": \"Alice\", \"age\": invalid }";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    /// Simulate a delayed stream where the JSON data arrives in two parts over time.
    #[tokio::test]
    async fn test_async_json_parser_delayed_stream() {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);

        tokio::spawn(async move {
            // Send the first half.
            tx.send(b"{ \"name\": \"A".to_vec()).await.unwrap();
            sleep(Duration::from_millis(100)).await;
            // Send the remainder.
            tx.send(b"lice\", \"age\": 30 }".to_vec()).await.unwrap();
        });

        let person: Person = parser.next().await.unwrap();
        assert_eq!(
            person,
            Person {
                name: "Alice".to_string(),
                age: 30
            }
        );
    }

    /// Parse multiple JSON objects sent one after another through a channel.
    #[tokio::test]
    async fn test_multiple_json_objects_in_stream() {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);

        // Spawn a task that sends two JSON objects in succession.
        tokio::spawn(async move {
            tx.send(b"{\"name\":\"Alice\", \"age\": 30}".to_vec())
                .await
                .unwrap();
            sleep(Duration::from_millis(50)).await;
            tx.send(b"{\"name\":\"Bob\", \"age\": 25}".to_vec())
                .await
                .unwrap();
        });

        let person1: Person = parser.next().await.unwrap();
        assert_eq!(
            person1,
            Person {
                name: "Alice".to_string(),
                age: 30
            }
        );

        let person2: Person = parser.next().await.unwrap();
        assert_eq!(
            person2,
            Person {
                name: "Bob".to_string(),
                age: 25
            }
        );
    }

    /// Return an UnexpectedEof error when the channel is closed without any data.
    #[tokio::test]
    async fn test_empty_channel() {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(1);
        drop(tx); // Close the channel immediately.
        let stream = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::UnexpectedEof);
    }

    /// Parse a large JSON object that is split across multiple chunks.
    #[tokio::test]
    async fn test_large_json_object_spanning_chunks() {
        let expected: Vec<i32> = (0..100).collect();
        let json_str = serde_json::to_string(&expected).unwrap();

        // Split the JSON string into two parts.
        let mid = json_str.len() / 2;
        let part1 = json_str[..mid].as_bytes().to_vec();
        let part2 = json_str[mid..].as_bytes().to_vec();

        let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);

        tokio::spawn(async move {
            tx.send(part1).await.unwrap();
            sleep(Duration::from_millis(50)).await;
            tx.send(part2).await.unwrap();
        });

        let result: Vec<i32> = parser.next().await.unwrap();
        assert_eq!(result, expected);
    }

    /// Parse JSON data that contains extra whitespace around it.
    #[tokio::test]
    async fn test_json_with_whitespace() {
        let data = b"   \n\t { \"name\": \"Alice\", \"age\": 30 }  \n ";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let person: Person = parser.next().await.unwrap();
        assert_eq!(
            person,
            Person {
                name: "Alice".to_string(),
                age: 30
            }
        );
    }
}
