use crate::streamer::Streamer;
use bmart_derive::EnumStr;
use hyper::{http, Body, Response, StatusCode};
use sha2::{Digest, Sha256};
use std::io::SeekFrom;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncSeekExt;

pub static DEFAULT_MIME_TYPE: &str = "application/octet-stream";

const TIME_STR: &str = "%a, %d %b %Y %T %Z";

#[derive(Debug, EnumStr, Copy, Clone, Eq, PartialEq)]
pub enum ErrorKind {
    Internal,
    Forbidden,
    NotFound,
    BadRequest,
}

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    source: Option<Box<dyn std::error::Error + 'static>>,
}

impl Error {
    #[inline]
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
    #[inline]
    pub fn bad_req() -> Self {
        Self {
            kind: ErrorKind::BadRequest,
            source: None,
        }
    }
    #[inline]
    pub fn forbidden() -> Self {
        Self {
            kind: ErrorKind::Forbidden,
            source: None,
        }
    }
    #[inline]
    pub fn internal(source: impl std::error::Error + 'static) -> Self {
        Self {
            kind: ErrorKind::Forbidden,
            source: Some(Box::new(source)),
        }
    }
}

impl From<Error> for Result<Response<Body>, http::Error> {
    fn from(err: Error) -> Self {
        let code = match err.kind() {
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorKind::Forbidden => StatusCode::FORBIDDEN,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
        };
        Response::builder().status(code).body(Body::empty())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error")
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(AsRef::as_ref)
    }
}

struct Range {
    start: u64,
    end: Option<u64>,
}

#[inline]
fn parse_range(range_hdr: &hyper::header::HeaderValue) -> Result<Range, Error> {
    let hdr = range_hdr.to_str().map_err(|_| Error::bad_req())?;
    let mut sp = hdr.splitn(2, '=');
    let units = sp.next().unwrap();
    if units == "bytes" {
        let range = sp.next().ok_or_else(Error::bad_req)?;
        let mut sp_range = range.splitn(2, '-');
        let start: u64 = sp_range
            .next()
            .unwrap()
            .parse()
            .map_err(|_| Error::bad_req())?;
        let end: Option<u64> = if let Some(end) = sp_range.next() {
            if end.is_empty() {
                None
            } else {
                Some(end.parse().map_err(|_| Error::bad_req())?)
            }
        } else {
            None
        };
        Ok(Range { start, end })
    } else {
        Err(Error::bad_req())
    }
}

#[inline]
fn etag_match(inm_hdr: &hyper::header::HeaderValue, etag: &str) -> Result<bool, Error> {
    let hdr = inm_hdr.to_str().map_err(|_| Error::bad_req())?;
    for t in hdr.split(',') {
        if t.trim() == etag {
            return Ok(true);
        }
    }
    Ok(false)
}

macro_rules! partial_resp {
    ($l: expr, $et: expr, $mt: expr) => {
        Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(hyper::header::ACCEPT_RANGES, "bytes")
            .header(
                hyper::header::LAST_MODIFIED,
                $l.with_timezone(&chrono_tz::GMT)
                    .format(TIME_STR)
                    .to_string(),
            )
            .header("ETag", $et)
            .header(
                hyper::header::CONTENT_TYPE,
                $mt.unwrap_or(DEFAULT_MIME_TYPE),
            )
    };
}

#[allow(clippy::too_many_lines)]
pub async fn static_file<'a>(
    file_path: &Path,
    mime_type: Option<&str>,
    headers: &hyper::header::HeaderMap,
    buf_size: usize,
) -> Result<Result<Response<Body>, http::Error>, Error> {
    macro_rules! forbidden {
        () => {
            return Err(Error::forbidden())
        };
    }
    macro_rules! int_error {
        ($err: expr) => {
            return Err(Error::internal($err))
        };
    }
    macro_rules! not_modified {
        () => {
            return Ok(Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .body(Body::empty()));
        };
    }
    let range = if let Some(range_hdr) = headers.get(hyper::header::RANGE) {
        Some(parse_range(range_hdr)?)
    } else {
        None
    };
    let (mut f, size, last_modified, etag) = match File::open(file_path).await {
        Ok(v) => {
            let (size, lmt) = match v.metadata().await {
                Ok(m) => {
                    if m.is_dir() {
                        forbidden!();
                    }
                    let last_modified = match m.modified() {
                        Ok(v) => v,
                        Err(e) => {
                            int_error!(e);
                        }
                    };
                    (m.len(), last_modified)
                }
                Err(e) => {
                    int_error!(e);
                }
            };
            let last_modified: chrono::DateTime<chrono::Utc> = lmt.into();
            let mut hasher = Sha256::new();
            hasher.update(file_path.to_string_lossy().as_bytes());
            hasher.update(last_modified.timestamp().to_le_bytes());
            hasher.update(last_modified.timestamp_subsec_nanos().to_le_bytes());
            (
                v,
                size,
                last_modified,
                format!(r#""{}""#, hex::encode(hasher.finalize())),
            )
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            forbidden!();
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error {
                kind: ErrorKind::NotFound,
                source: None,
            });
        }
        Err(e) => {
            int_error!(e);
        }
    };
    if let Some(h) = headers.get(hyper::header::IF_NONE_MATCH) {
        if etag_match(h, &etag)? {
            not_modified!();
        }
    } else if let Some(h) = headers.get(hyper::header::IF_MODIFIED_SINCE) {
        let hdr = h.to_str().map_err(|_| Error::bad_req())?;
        let dt = chrono::DateTime::parse_from_rfc2822(hdr).map_err(|_| Error::bad_req())?;
        if last_modified.timestamp() == dt.timestamp() {
            not_modified!();
        }
    }
    Ok(if let Some(rn) = range {
        if rn.end.map_or_else(|| rn.start < size, |v| v >= rn.start)
            && f.seek(SeekFrom::Start(rn.start)).await.is_ok()
        {
            let part_size = rn
                .end
                .map_or_else(|| size - rn.start, |end| end - rn.start + 1);
            let reader = Streamer::new(f, buf_size);
            partial_resp!(last_modified, etag, mime_type)
                .header(
                    hyper::header::CONTENT_RANGE,
                    format!("bytes {}-{}/{}", rn.start, rn.end.unwrap_or(size - 1), size),
                )
                .header(hyper::header::CONTENT_LENGTH, part_size)
                .body(Body::wrap_stream(reader.into_stream_sized(part_size)))
        } else {
            Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(hyper::header::ACCEPT_RANGES, "bytes")
                .header(hyper::header::CONTENT_RANGE, format!("*/{}", size))
                .body(Body::empty())
        }
    } else {
        let reader = Streamer::new(f, buf_size);
        partial_resp!(last_modified, etag, mime_type)
            .header(hyper::header::CONTENT_LENGTH, size)
            .body(Body::wrap_stream(reader.into_stream()))
    })
}
