use hyper::{Request, Response, StatusCode, body::Incoming, HeaderMap};
use hyper::service::service_fn;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use bytes::Bytes;
use futures_util::StreamExt;
use tokio::net::TcpListener;
use tokio::fs::{self, File};
use tokio::io::AsyncSeekExt;
use tokio_util::io::ReaderStream;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::io::SeekFrom;
use percent_encoding::percent_decode_str;
use anyhow::Result;

// Configuration for mapping directory names to paths
#[derive(Clone, Debug)]
pub struct ServerConfig {
    directories: HashMap<String, PathBuf>,
}

impl ServerConfig {
    pub fn new(directories: HashMap<String, PathBuf>) -> Self {
        ServerConfig { directories }
    }
}

// HTML generation utilities
mod html {
    use super::*;

    pub fn directory_index(directories: &HashMap<String, PathBuf>) -> String {
        let mut html = String::from("<!DOCTYPE html>\n<html>\n<head><title>Directory Index</title></head>\n<body>\n<h1>Available Directories</h1>\n<ul>\n");
        for name in directories.keys() {
            html.push_str(&format!("<li><a href=\"/{}/\">{}</a></li>\n", name, name));
        }
        html.push_str("</ul>\n</body>\n</html>");
        html
    }

    pub fn directory_listing(full_path: &str, entries: Vec<String>) -> String {
        let mut html = String::from("<!DOCTYPE html>\n<html><body><h1>Directory Contents</h1><ul>\n");
        for name in entries {
            let link_path = if full_path.ends_with('/') {
                format!("{}{}", full_path, name)
            } else {
                format!("{}/{}", full_path, name)
            };
            html.push_str(&format!("<li><a href=\"{}\">{}</a></li>\n", link_path, name));
        }
        html.push_str("</ul></body></html>");
        html
    }
}

// Request handling logic
mod handler {
    use super::*;

    pub async fn handle_request(req: Request<Incoming>, config: ServerConfig) -> Result<Response<BoxBody<Bytes, std::io::Error>>, Infallible> {
        let path = req.uri().path();
        log::info!("Handling request for: {}", path);

        if path == "/" || path == "" {
            log::info!("Serving directory index - Status: 200 OK");
            return Ok(build_ok_response("text/html", html::directory_index(&config.directories)));
        }

        let clean_path = percent_decode_str(path).decode_utf8_lossy().trim_start_matches('/').to_string();
        let (base, subpath) = split_path(&clean_path);

        if let Some(dir_path) = config.directories.get(base) {
            if let Some(response) = try_serve_path(dir_path, base, subpath, req.headers()).await {
                return Ok(response);
            }
            log::debug!("Not found in mapped dir: {:?} - Status: 404 Not Found", dir_path.join(subpath));
        }

        log::info!("Path not found: {} - Status: 404 Not Found", path);
        Ok(build_not_found_response())
    }

    fn split_path(path: &str) -> (&str, &str) {
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        (parts.get(0).unwrap_or(&""), parts.get(1).unwrap_or(&""))
    }

    async fn try_serve_path(dir_path: &Path, base: &str, subpath: &str, headers: &HeaderMap) -> Option<Response<BoxBody<Bytes, std::io::Error>>> {
        let full_path = dir_path.join(subpath);
        log::debug!("Checking: {:?}", full_path);

        if !full_path.exists() {
            return None;
        }

        if full_path.is_file() {
            serve_file(&full_path, headers).await
        } else if full_path.is_dir() {
            let request_path = if subpath.is_empty() {
                format!("/{}", base)
            } else {
                format!("/{}/{}", base, subpath)
            };
            serve_directory(&full_path, &request_path).await
        } else {
            None
        }
    }

    async fn serve_file(path: &Path, headers: &HeaderMap) -> Option<Response<BoxBody<Bytes, std::io::Error>>> {
        match File::open(path).await {
            Ok(mut file) => {
                let metadata = fs::metadata(path).await.ok()?;
                let file_size = metadata.len();

                // Parse Range header
                if let Some(range) = headers.get("Range") {
                    if let Ok(range_str) = range.to_str() {
                        if let Some((start, end)) = parse_range(range_str, file_size) {
                            // Serve partial content
                            let content_length = end - start + 1;
                            let content_length_usize = content_length.try_into().expect("Content length too large for usize");
                            file.seek(SeekFrom::Start(start)).await.ok()?;
                            let reader_stream = ReaderStream::new(file).take(content_length_usize);
                            let stream_body = StreamBody::new(reader_stream.map(|res| res.map(hyper::body::Frame::data)));

                            log::info!("Serving partial file: {:?} (bytes {}-{}, {} bytes) - Status: 206 Partial Content",
                                      path, start, end, content_length);
                            return Some(Response::builder()
                                .status(StatusCode::PARTIAL_CONTENT)
                                .header("Content-Type", "application/octet-stream")
                                .header("Content-Length", content_length.to_string())
                                .header("Content-Range", format!("bytes {}-{}/{}", start, end, file_size))
                                .header("Accept-Ranges", "bytes")
                                .body(BoxBody::new(stream_body))
                                .unwrap());
                        }
                    }
                }

                // Serve full file if no valid Range header
                log::info!("Serving full file: {:?} ({} bytes) - Status: 200 OK", path, file_size);
                let reader_stream = ReaderStream::new(file);
                let stream_body = StreamBody::new(reader_stream.map(|res| res.map(hyper::body::Frame::data)));
                Some(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/octet-stream")
                    .header("Content-Length", file_size.to_string())
                    .header("Accept-Ranges", "bytes")
                    .body(BoxBody::new(stream_body))
                    .unwrap())
            }
            Err(e) => {
                log::warn!("Error opening file {:?}: {} - Status: 500 Internal Server Error", path, e);
                None
            }
        }
    }

    fn parse_range(range: &str, file_size: u64) -> Option<(u64, u64)> {
        if !range.starts_with("bytes=") {
            return None;
        }
        let range = &range[6..]; // Strip "bytes="
        let parts: Vec<&str> = range.splitn(2, '-').collect();
        if parts.len() != 2 {
            return None;
        }

        let start = parts[0].trim().parse::<u64>().ok()?;
        let end = if parts[1].trim().is_empty() {
            file_size - 1
        } else {
            parts[1].trim().parse::<u64>().ok()?
        };

        if start > end || end >= file_size {
            return None;
        }
        Some((start, end))
    }

    async fn serve_directory(path: &Path, request_path: &str) -> Option<Response<BoxBody<Bytes, std::io::Error>>> {
        match read_dir_entries(path).await {
            Ok(entries) => {
                log::info!("Serving directory listing: {:?}", path);
                let html = html::directory_listing(request_path, entries);
                Some(build_ok_response("text/html", html))
            }
            Err(e) => {
                log::warn!("Error reading directory {:?}: {} - Status: 500 Internal Server Error", path, e);
                None
            }
        }
    }

    async fn read_dir_entries(dir: &Path) -> Result<Vec<String>, std::io::Error> {
        let mut entries = Vec::new();
        let mut dir_entries = fs::read_dir(dir).await?;
        while let Some(entry) = dir_entries.next_entry().await? {
            entries.push(entry.file_name().to_string_lossy().to_string());
        }
        Ok(entries)
    }

    fn build_ok_response(content_type: &str, body: String) -> Response<BoxBody<Bytes, std::io::Error>> {
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", content_type)
            .body(Full::new(Bytes::from(body)).map_err(|e| match e {}).boxed())
            .unwrap()
    }

    fn build_not_found_response() -> Response<BoxBody<Bytes, std::io::Error>> {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "text/plain")
            .body(Full::new(Bytes::from_static(b"404 - Not Found")).map_err(|e| match e {}).boxed())
            .unwrap()
    }
}

pub fn parse_config(dirs: Vec<String>) -> Result<ServerConfig, String> {
    let mut directories = HashMap::new();

    for dir_spec in dirs {
        let (name, path) = match dir_spec.split_once(':') {
            Some((n, p)) => (n.to_string(), p.to_string()),
            None => (dir_spec.clone(), dir_spec),
        };

        let path_buf = PathBuf::from(&path);
        if !path_buf.exists() {
            log::warn!("Directory '{}' for '{}' does not exist", path, name);
        } else if !path_buf.is_dir() {
            return Err(format!("'{}' is not a directory", path));
        }

        directories.insert(name, path_buf);
    }

    if directories.is_empty() {
        return Err("No valid directories specified".to_string());
    }

    Ok(ServerConfig { directories })
}

pub async fn run_server(config: ServerConfig, port: u16) -> Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    log::info!("Listening on http://{}", addr);
    display_directories(&config);

    let listener = TcpListener::bind(addr).await?;
    let config_clone = config.clone();

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let config = config_clone.clone();

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(move |req| handler::handle_request(req, config.clone())))
                .await
            {
                log::error!("Error serving connection: {:?}", err);
            }
        });
    }
}

fn display_directories(config: &ServerConfig) {
    log::info!("Serving directories:");
    for (name, path) in &config.directories {
        log::info!("  /{name} -> {}", path.display());
    }
}
