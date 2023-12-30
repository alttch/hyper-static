# hyper-static - a static file handler for Rust/Hyper with minimal logic

* Crate: <https://crates.io/crates/hyper-static>
* Repository: <https://github.com/alttch/hyper-static>

The idea is to have a static file handler with no overhead. Make any handler
function for Hyper, with own logic, if a static file needs to be returned -
give the crate a path and that is it.

* serves large files with zero-copy buffers
* correctly handles partial requests (Content-Range)
* handles If-Modified-Since, If-None-Match
* crate errors can be transformed directly into Hyper results

Example:

```rust,ignore
use hyper_static::serve::static_file;

// define some hyper handler
async fn handler(req: Request<Incoming>) -> Result<ResponseStreamed, http::Error> {
    // ....
    // serve a file when necessary
    // in a simple way
    let mut path = std::path::Path::new("/path/to/files").to_owned();
    path.push(&req.uri().path()[1..]);
    return match static_file(
        &path,
        Some("application/octet-stream"), // mime type
        &req.headers(),                   // hyper request header map
        65536,                            // buffer size
    )
    .await
    {
        Ok(v) => v,         // return it
        Err(e) => e.into(), // transform the error and return
    };
    //more complicated - analyze errors, e.g. log them
    return match static_file(
        &path,
        Some("application/octet-stream"),
        &req.headers(),
        65536,
    )
    .await
    {
        Ok(v) => {
            println!(
                r#""GET {}" {}"#,
                req.uri(),
                v.as_ref().map_or(0, |res| res.status().as_u16())
            );
            v
        }
        Err(e) => {
            let resp: Result<ResponseStreamed, http::Error> = e.into();
            eprintln!(
                r#""GET {}" {}"#,
                req.uri(),
                resp.as_ref().map_or(0, |res| res.status().as_u16())
            );
            resp
        }
    };
}
```
