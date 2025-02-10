use std::io::Error as IoError;
use tokio::io::{AsyncRead, ReadBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

/// A custom asynchronous reader that receives data in chunks from a Tokio mpsc channel.
/// This simulates a streaming source (for example, network data or an LLM response delivered in parts).
pub struct ChannelReader {
    rx: mpsc::Receiver<Vec<u8>>,
    buffer: Vec<u8>,
}

impl ChannelReader {
    /// Creates a new `ChannelReader` from the given mpsc receiver.
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
        self.buffer.drain(0..n);
        Poll::Ready(Ok(()))
    }
}
