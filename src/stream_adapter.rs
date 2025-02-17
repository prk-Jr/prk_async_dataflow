use bytes::{Buf, BufMut, BytesMut};
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

pub struct StreamToAsyncRead<S> {
    stream: S,
    buffer: BytesMut,
}

impl<S> StreamToAsyncRead<S> {
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            buffer: BytesMut::new(),
        }
    }
}

impl<S> AsyncRead for StreamToAsyncRead<S>
where
    S: Stream<Item = Result<Vec<u8>, std::io::Error>> + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.buffer.is_empty() {
            match Pin::new(&mut self.stream).poll_next(cx) {
                Poll::Ready(Some(Ok(data))) => {
                    self.buffer.put_slice(&data);
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
                Poll::Ready(None) => return Poll::Ready(Ok(())), // EOF
                Poll::Pending => return Poll::Pending,
            }
        }

        let len = std::cmp::min(self.buffer.len(), buf.remaining());
        buf.put_slice(&self.buffer[..len]);
        self.buffer.advance(len);
        Poll::Ready(Ok(()))
    }
}