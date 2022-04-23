# hyper-static - a static file handler for Rust/Hyper with minimal logic

Crate: <https://crates.io/crates/hyper-static>

Repository: <https://github.com/alttch/hyper-static>

The idea is to have a static file handler with no overhead. Make any handler
function for Hyper, with own logic, if a static file needs to be returned -
give the crate a path and that is it.

* serves large files with zero-copy buffers
* correctly processes partial requests (Content-Range)
* crate errors can be transformed directly into Hyper results

Example:

```rust,ignore
use hyper_static::serve::static_file;

// define some hyper handler
async fn handler(req: Request<Body>) -> Result<Response<Body>, http::Error> {
    // ....
    // serve a file when necessary
    // in a simple way
    let path = std::path::Path::new("/path/to/file");
    return match static_file(
        &path,
        Some("text/html"), // mime type
        &req.headers(), // hyper request header map
        65536 // buffer size
    )
    .await
    {
        Ok(v) => v, // return it
        Err(e) => e.into(), // transform the error and return
    };
    // more complicated - analyze errors, e.g. log them
    return match static_file(
        &path,
        Some("text/html"),
        &parts.headers,
        65536
    )
    .await
    {
        Ok(v) => {
            debug!(
                r#""GET {}" {}"#,
                uri,
                v.as_ref().map_or(0, |res| res.status().as_u16())
            );
            v
        }
        Err(e) => {
            let resp: Result<Response<Body>, http::Error> = e.into();
            warn!(
                r#""GET {}" {}"#,
                uri,
                resp.as_ref().map_or(0, |res| res.status().as_u16())
            );
            resp
        }
    };
}
```
