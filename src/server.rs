use actix_web::{App, HttpServer, middleware::Logger, HttpResponse, Responder, web, http::header, HttpRequest};
use actix_files::Files;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;
use log::debug;
use percent_encoding::percent_decode_str;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    directories: HashMap<String, PathBuf>,
}

impl ServerConfig {
    pub fn new(directories: HashMap<String, PathBuf>) -> Self {
        ServerConfig { directories }
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

async fn root_index(config: web::Data<ServerConfig>) -> impl Responder {
    let mut dir_names: Vec<&String> = config.directories.keys().collect();
    dir_names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase())); // Case-insensitive sort

    let mut html = String::from("<!DOCTYPE html>\n<html>\n<head><title>FPKGi Server Index</title></head>\n<body>\n<h1>Available Directories</h1>\n<ul>\n");
    for name in dir_names {
        html.push_str(&format!(
            "<li><a href=\"/{}/\">/{}</a></li>\n",
            name, name
        ));
    }
    html.push_str("</ul>\n</body>\n</html>");
    HttpResponse::Ok()
        .content_type("text/html")
        .body(html)
}

async fn dir_listing(config: web::Data<ServerConfig>, req: HttpRequest) -> impl Responder {
    let path_str = req.path();
    let clean_path = path_str.trim_start_matches('/');
    let decoded_path = percent_decode_str(clean_path).decode_utf8_lossy().to_string();

    debug!("Dir listing requested: {} (decoded: {})", clean_path, decoded_path);

    let (base, subpath) = match decoded_path.split_once('/') {
        Some((b, s)) => (b.to_string(), s.trim_end_matches('/').to_string()),
        None => (decoded_path.trim_end_matches('/').to_string(), String::new()),
    };

    if let Some(dir_path) = config.directories.get(&base) {
        let full_path = dir_path.join(&subpath);
        if full_path.is_dir() {
            match fs::read_dir(&full_path) {
                Ok(entries) => {
                    let mut file_list: Vec<String> = entries
                        .filter_map(|entry| entry.ok())
                        .map(|entry| entry.file_name().to_string_lossy().to_string())
                        .collect();
                    file_list.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase())); // Case-insensitive sort

                    let request_path = format!("/{}", clean_path); // Use original encoded path for links
                    let mut html = String::from("<!DOCTYPE html>\n<html>\n<head><title>Directory Listing</title></head>\n<body>\n<h1>Directory Contents</h1>\n<ul>\n");
                    for name in file_list {
                        let link_path = format!("{}/{}", request_path.trim_end_matches('/'), name);
                        html.push_str(&format!("<li><a href=\"{}\">{}</a></li>\n", link_path, name));
                    }
                    html.push_str("</ul>\n</body>\n</html>");
                    debug!("Rendering directory listing for: {}", clean_path);
                    return HttpResponse::Ok()
                        .content_type("text/html")
                        .body(html);
                }
                Err(e) => {
                    log::warn!("Error reading directory {:?}: {}", full_path, e);
                    return HttpResponse::InternalServerError().body("Error reading directory");
                }
            }
        }
    }

    debug!("Not a directory, falling back to Files: {}", clean_path);
    HttpResponse::NotFound().body("404 - Not Found")
}

async fn dir_redirect(config: web::Data<ServerConfig>, req: HttpRequest) -> impl Responder {
    let path_str = req.path();
    let clean_path = path_str.trim_start_matches('/');
    let decoded_path = percent_decode_str(clean_path).decode_utf8_lossy().to_string();

    debug!("Dir redirect requested: {} (decoded: {})", clean_path, decoded_path);

    let (base, subpath) = match decoded_path.split_once('/') {
        Some((b, s)) => (b.to_string(), s.to_string()),
        None => (decoded_path.to_string(), String::new()),
    };

    if let Some(dir_path) = config.directories.get(&base) {
        let full_path = dir_path.join(&subpath);
        if full_path.is_dir() {
            let redirect_path = format!("/{}", clean_path); // Use original encoded path for redirect
            debug!("Redirecting to: {}/", redirect_path);
            return HttpResponse::PermanentRedirect()
                .append_header((header::LOCATION, format!("{}/", redirect_path)))
                .finish();
        }
    }

    debug!("Not a directory, falling back to Files: {}", clean_path);
    HttpResponse::NotFound().body("404 - Not Found")
}

pub async fn run_server(config: ServerConfig, port: u16) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    log::info!("Listening on http://{}", addr);
    display_directories(&config);

    let config_clone = config.clone();
    let directories = config.directories.clone();
    HttpServer::new(move || {
        let mut app = App::new()
            .wrap(Logger::default()) // Access logging middleware
            .app_data(web::Data::new(config_clone.clone())) // Share config with handlers
            .route("/", web::get().to(root_index)); // Root index handler

        // Register specific directory routes
        for name in directories.keys() {
            app = app.service(web::resource(&format!("/{}/", name)).route(web::get().to(dir_listing)));
            app = app.service(web::resource(&format!("/{}", name)).route(web::get().to(dir_redirect)));
            // Register subfolder routes dynamically
            if let Ok(entries) = fs::read_dir(&directories[name]) {
                for entry in entries.filter_map(Result::ok) {
                    if entry.path().is_dir() {
                        let subpath = entry.file_name().to_string_lossy().to_string();
                        let dir_with_slash = format!("/{}/{}/", name, subpath);
                        let dir_without_slash = format!("/{}/{}", name, subpath);
                        app = app.service(web::resource(dir_with_slash).route(web::get().to(dir_listing)));
                        app = app.service(web::resource(dir_without_slash).route(web::get().to(dir_redirect)));
                    }
                }
            }
        }

        // File serving with actix-files after specific routes
        for (name, path) in &config_clone.directories {
            app = app.service(
                Files::new(&format!("/{}", name), path)
                    .prefer_utf8(true) // Ensure proper encoding handling
                    .use_last_modified(true) // Last-Modified header
                    .use_etag(true) // ETag support
            );
        }

        app
    })
    .bind(&addr)?
    .run()
    .await
    .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

    Ok(())
}

fn display_directories(config: &ServerConfig) {
    log::info!("Serving directories:");
    for (name, path) in &config.directories {
        log::info!("  /{name} -> {}", path.display());
    }
}
