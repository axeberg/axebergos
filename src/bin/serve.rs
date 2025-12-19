//! Development server for axeberg
//!
//! A minimal static file server. No dependencies beyond tiny_http.
//! Comprehensible in one sitting.

use std::fs;
use std::path::Path;
use tiny_http::{Header, Response, Server};

const DEFAULT_PORT: u16 = 8080;

fn main() {
    let port = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let addr = format!("0.0.0.0:{}", port);
    let server = Server::http(&addr).expect("Failed to start server");

    println!("┌─────────────────────────────────────┐");
    println!("│  axeberg dev server                 │");
    println!("├─────────────────────────────────────┤");
    println!("│  http://localhost:{}              │", port);
    println!("└─────────────────────────────────────┘");

    for request in server.incoming_requests() {
        let url_path = request.url().to_string();
        let file_path = if url_path == "/" {
            "index.html".to_string()
        } else {
            url_path.trim_start_matches('/').to_string()
        };

        let response = serve_file(&file_path);
        let _ = request.respond(response);
    }
}

fn serve_file(path: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let path = Path::new(path);

    match fs::read(path) {
        Ok(contents) => {
            let mime = mime_type(path);
            let header = Header::from_bytes("Content-Type", mime).unwrap();
            Response::from_data(contents).with_header(header)
        }
        Err(_) => Response::from_string("404 Not Found")
            .with_status_code(404)
            .with_header(Header::from_bytes("Content-Type", "text/plain").unwrap()),
    }
}

fn mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}
