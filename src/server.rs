use actix_web::{App, HttpServer, middleware::Logger, HttpResponse, Responder, web};
use actix_files::Files;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

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
    let mut html = String::from("<!DOCTYPE html>\n<html>\n<head><title>FPKGi Server Index</title></head>\n<body>\n<h1>Available Directories</h1>\n<ul>\n");
    for name in config.directories.keys() {
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

pub async fn run_server(config: ServerConfig, port: u16) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    log::info!("Listening on http://{}", addr);
    display_directories(&config);

    let config_clone = config.clone();
    HttpServer::new(move || {
        let mut app = App::new()
            .wrap(Logger::default()) // Access logging middleware
            .app_data(web::Data::new(config_clone.clone())) // Share config with handlers
            .route("/", web::get().to(root_index)); // Root index handler
        for (name, path) in &config_clone.directories {
            app = app.service(
                Files::new(&format!("/{}", name), path)
                    .show_files_listing()
                    .use_last_modified(true)
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
