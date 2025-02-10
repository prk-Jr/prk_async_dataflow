use serde::de::DeserializeOwned;
use serde_json::{self, error::Category};
use std::io::{Error as IoError, ErrorKind};
use tokio::io::{AsyncRead, AsyncReadExt, Error, ReadBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

/// Extracts the first JSON block (balanced braces or brackets) from a text string.
/// It supports both JSON objects (starting with '{') and JSON arrays (starting with '[').
/// This function ignores braces/brackets that occur inside string literals.
fn extract_json(text: &str) -> Option<String> {
    let text = text.trim();
    // If the text starts with a valid JSON start char, use it; otherwise, search for one.
    let first_char = text.chars().next()?;
    let (opening, closing) = match first_char {
        '{' => ('{', '}'),
        '[' => ('[', ']'),
        _ => {
            // If not starting with { or [, try to find the first occurrence of either.
            let pos1 = text.find('{');
            let pos2 = text.find('[');
            let start = match (pos1, pos2) {
                (Some(a), Some(b)) => std::cmp::min(a, b),
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => return None,
            };
            return extract_json(&text[start..]);
        }
    };

    let mut count = 0;
    let mut in_string = false;
    let mut escape = false;
    let mut end = None;

    for (i, c) in text.char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
        } else {
            if c == '"' {
                in_string = true;
            } else if c == opening {
                count += 1;
            } else if c == closing {
                count -= 1;
                if count == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
        }
    }
    end.map(|e| text[..e].to_string())
}

/// A streaming JSON parser that buffers incoming data and, once available,
/// extracts a JSON block (even if extra text is present) and deserializes it.
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

    /// Reads data into an internal buffer until a valid JSON block can be extracted
    /// (using `extract_json`), then attempts to deserialize that block into type T.
    pub async fn next<T: DeserializeOwned>(&mut self) -> Result<T, Error> {
        let mut chunk = [0u8; 1024];

        loop {
            // Attempt to convert the current buffer to a UTF-8 string.
            if let Ok(text) = std::str::from_utf8(&self.buffer) {
                if let Some(json_str) = extract_json(text) {
                    match serde_json::from_str::<T>(&json_str) {
                        Ok(value) => {
                            // Clear the buffer once successfully parsed.
                            self.buffer.clear();
                            return Ok(value);
                        }
                        // If the error indicates incomplete JSON, we continue reading.
                        Err(e) if e.classify() == Category::Eof => {}
                        // Otherwise, if we get an error, we ignore it and read more.
                        Err(_) => {}
                    }
                }
            }
            // Read more data from the underlying stream.
            let bytes_read = self.reader.read(&mut chunk).await?;
            if bytes_read == 0 {
                // No more data: make one last attempt at extraction.
                if let Ok(text) = std::str::from_utf8(&self.buffer) {
                    if let Some(json_str) = extract_json(text) {
                        return serde_json::from_str::<T>(&json_str)
                            .map_err(|e| Error::new(ErrorKind::InvalidData, e));
                    }
                }
                return Err(Error::new(ErrorKind::UnexpectedEof, "Incomplete JSON data"));
            }
            self.buffer.extend_from_slice(&chunk[..bytes_read]);
        }
    }
}

/// A custom asynchronous reader that receives data in chunks from a Tokio mpsc channel.
/// This simulates a streaming source.
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
        if self.buffer.is_empty() {
            match Pin::new(&mut self.rx).poll_recv(cx) {
                Poll::Ready(Some(chunk)) => {
                    self.buffer = chunk;
                }
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
        let remaining = buf.remaining();
        let n = std::cmp::min(remaining, self.buffer.len());
        buf.put_slice(&self.buffer[..n]);
        self.buffer.drain(..n);
        Poll::Ready(Ok(()))
    }
}

/// Main function simulating an AI agent that receives an LLM response containing extra text
/// along with a JSON payload. The code extracts and parses the JSON into a Rust struct.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Define the expected data structure from the LLM response.
    #[derive(Debug, serde::Deserialize)]
    struct Data {
        name: String,
        age: u32,
        roles: Vec<String>,
    }
    #[derive(Debug, serde::Deserialize)]
    struct LLMResponse {
        status: String,
        message: String,
        data: Data,
        timestamp: String,
    }

    // Simulate an LLM response with extra text surrounding the JSON.
    let llm_response = r#"Certainly! Here's a simple JSON response example:

json
Copy
[{
  "status": "success",
  "message": "Data retrieved successfully",
  "data": {
    "id": 123,
    "name": "John Doe",
    "email": "johndoe@example.com",
    "age": 30,
    "isActive": true,
    "roles": ["user", "admin"],
    "preferences": {
      "theme": "dark",
      "notifications": true
    }
  },
  "timestamp": "2023-10-05T12:34:56Z"
}]
This JSON response includes a status, a message, some data about a user, and a timestamp. Let me know if you need a more specific or customized JSON response!"#;

    // Simulate streaming by splitting the response into chunks.
    let parts: Vec<Vec<u8>> = llm_response
        .as_bytes()
        .chunks(50)
        .map(|chunk| chunk.to_vec())
        .collect();

    let (tx, rx) = mpsc::channel::<Vec<u8>>(10);
    let reader = ChannelReader::new(rx);
    let mut parser = AsyncJsonParser::new(reader);

    // Spawn a background task that sends the parts with a small delay.
    tokio::spawn(async move {
        for part in parts {
            tx.send(part).await.unwrap();
            sleep(Duration::from_millis(50)).await;
        }
    });

    println!("Waiting for the LLM response and extracting the JSON part...");
    let response: Vec<LLMResponse> = parser.next().await?;
    println!("Parsed LLM response:\n{:#?}", response);

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

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Data {
        id: u32,
        name: String,
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct LLMResponse {
        status: String,
        message: String,
        data: Data,
        timestamp: String,
    }

    // --- Tests for extract_json function ---

    #[test]
    fn test_extract_json_valid() {
        let text = "Some header text { \"key\": \"value\" } some footer text";
        let extracted = extract_json(text).unwrap();
        assert_eq!(extracted, "{ \"key\": \"value\" }");
    }

    #[test]
    fn test_extract_json_no_json() {
        let text = "There is no JSON here.";
        assert!(extract_json(text).is_none());
    }

    #[test]
    fn test_extract_json_multiple_json() {
        let text = "Pretext { \"first\": 1 } middle { \"second\": 2 } end";
        let extracted = extract_json(text).unwrap();
        assert_eq!(extracted, "{ \"first\": 1 }");
    }

    #[test]
    fn test_extract_json_with_escaped_quotes() {
        let text = r#"Prefix { "key": "a \"quoted\" value" } Suffix"#;
        let extracted = extract_json(text).unwrap();
        assert_eq!(extracted, r#"{ "key": "a \"quoted\" value" }"#);
    }

    // --- Tests for AsyncJsonParser with pure JSON ---

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

    #[tokio::test]
    async fn test_async_json_parser_incomplete_data() {
        let data = b"{ \"name\": \"Alice\", \"age\": ";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn test_async_json_parser_invalid_data() {
        let data = b"{ \"name\": \"Alice\", \"age\": invalid }";
        let stream = BufReader::new(Cursor::new(data.to_vec()));
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn test_async_json_parser_delayed_stream() {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);

        tokio::spawn(async move {
            tx.send(b"{ \"name\": \"A".to_vec()).await.unwrap();
            sleep(Duration::from_millis(100)).await;
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

    #[tokio::test]
    async fn test_multiple_json_objects_in_stream() {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(2);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);

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

    #[tokio::test]
    async fn test_empty_channel() {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(1);
        drop(tx);
        let stream = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(stream);
        let result: Result<Person, _> = parser.next().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn test_large_json_object_spanning_chunks() {
        let expected: Vec<i32> = (0..100).collect();
        let json_str = serde_json::to_string(&expected).unwrap();
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

    // --- Tests for LLM response extraction (extra text around JSON) ---

    #[tokio::test]
    async fn test_llm_response_extraction() {
        let llm_response = r#"Certainly! Here's a simple JSON response example:

json
Copy
{
  "status": "success",
  "message": "Data retrieved successfully",
  "data": {
    "id": 123,
    "name": "John Doe",
    "email": "johndoe@example.com",
    "age": 30,
    "isActive": true,
    "roles": ["user", "admin"],
    "preferences": {
      "theme": "dark",
      "notifications": true
    }
  },
  "timestamp": "2023-10-05T12:34:56Z"
}
This JSON response includes a status, a message, some data about a user, and a timestamp. Let me know if you need a more specific or customized JSON response!"#;

        let parts: Vec<Vec<u8>> = llm_response
            .as_bytes()
            .chunks(30)
            .map(|chunk| chunk.to_vec())
            .collect();

        let (tx, rx) = mpsc::channel::<Vec<u8>>(10);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);

        tokio::spawn(async move {
            for part in parts {
                tx.send(part).await.unwrap();
                sleep(Duration::from_millis(10)).await;
            }
        });

        let response: LLMResponse = parser.next().await.unwrap();
        assert_eq!(response.status, "success");
        assert_eq!(response.data.name, "John Doe");
    }

    #[tokio::test]
    async fn test_async_json_parser_with_extra_text() {
        // Simulate extra text before and after the JSON.
        let text = r#"Some header info...
Here is the JSON:
{
  "name": "Alice",
  "age": 30
}
And here is some footer text."#;
        let parts: Vec<Vec<u8>> = text
            .as_bytes()
            .chunks(40)
            .map(|chunk| chunk.to_vec())
            .collect();
        let (tx, rx) = mpsc::channel::<Vec<u8>>(10);
        let reader = ChannelReader::new(rx);
        let mut parser = AsyncJsonParser::new(reader);

        tokio::spawn(async move {
            for part in parts {
                tx.send(part).await.unwrap();
                sleep(Duration::from_millis(20)).await;
            }
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
}
