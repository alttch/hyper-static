use async_stream::stream;
use futures::{Stream, StreamExt};
use hyper::body::{Body, Buf, Frame, SizeHint};
use std::collections::VecDeque;
use std::io::Error;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Default)]
pub struct Empty<D> {
    _marker: PhantomData<fn() -> D>,
}

impl<D> Empty<D>
where
    D: Default,
{
    pub fn new() -> Self {
        Self::default()
    }
}

impl<D: Buf> Body for Empty<D> {
    type Data = D;
    type Error = Error;

    #[inline]
    fn poll_frame(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(None)
    }

    fn is_end_stream(&self) -> bool {
        true
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(0)
    }
}

pub struct Streamer<R>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    reader: R,
    buf_size: usize,
}

impl<R> Streamer<R>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    #[inline]
    pub fn new(reader: R, buf_size: usize) -> Self {
        Self { reader, buf_size }
    }
    pub fn into_stream(
        mut self,
    ) -> Pin<Box<impl ?Sized + Stream<Item = Result<Frame<VecDeque<u8>>, Error>> + 'static>> {
        let stream = stream! {
            loop {
                let mut buf = vec![0; self.buf_size];
                let r = self.reader.read(&mut buf).await?;
                if r == 0 {
                    break
                }
                buf.truncate(r);
                yield Ok(Frame::data(buf.into()));
            }
        };
        stream.boxed()
    }
    // allow truncation as truncated remaining is always less than buf_size: usize
    #[allow(clippy::cast_possible_truncation)]
    pub fn into_stream_sized(
        mut self,
        max_length: u64,
    ) -> Pin<Box<impl ?Sized + Stream<Item = Result<Frame<VecDeque<u8>>, Error>> + 'static>> {
        let stream = stream! {
            let mut remaining = max_length;
            loop {
                if remaining == 0 {
                    break;
                }
                let bs = if remaining >= self.buf_size as u64 {
                    self.buf_size
                } else {
                    remaining as usize
                };
                let mut buf = vec![0; bs];
                let r = self.reader.read(&mut buf).await?;
                if r == 0 {
                    break;
                }
                buf.truncate(r);
                yield Ok(Frame::data(buf.into()));
                remaining -= r as u64;
            }
        };
        stream.boxed()
    }
}
