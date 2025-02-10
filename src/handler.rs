use mime_guess::from_path;
use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::Arc,
};
use urlencoding::decode;

// Sanitize requested path to prevent directory traversal
fn sanitize_path(base_dir: &Path, requested_path: &str, index_file: &str) -> Option<PathBuf> {
    if base_dir.as_os_str().is_empty() || index_file.is_empty() {
        return None;
    }

    // Decode URL-encoded path
    let requested_path = decode(requested_path).ok()?.trim().to_string();

    // Default to index file if root is requested
    let target_path = if requested_path == "/" || requested_path.is_empty() {
        base_dir.join(index_file)
    } else {
        base_dir.join(requested_path.trim_start_matches('/'))
    };

    // Resolve canonical path and ensure it stays within base directory
    match target_path.canonicalize() {
        Ok(clean_path) if clean_path.starts_with(base_dir) && clean_path.is_file() => {
            Some(clean_path)
        }
        _ => None,
    }
}

// Send an HTTP response
fn send_response(stream: &mut TcpStream, status: &str, content: Option<&[u8]>, content_type: &str) {
    let content_length = content.map_or(0, |c| c.len());

    // Build response headers
    let response_headers = format!(
        "HTTP/1.1 {}\r\n\
        Content-Type: {}\r\n\
        Content-Length: {}\r\n\
        Connection: close\r\n\
        \r\n",
        status, content_type, content_length
    );

    // Write headers to the client
    if let Err(e) = stream.write_all(response_headers.as_bytes()) {
        eprintln!("Failed to send response headers: {}", e);
        return;
    }

    // Write content if available
    if let Some(body) = content {
        if let Err(e) = stream.write_all(body) {
            eprintln!("Failed to send response body: {}", e);
        }
    }
}

// Handle a single HTTP request
pub fn handle_client(mut stream: TcpStream, base_dir: Arc<PathBuf>, index_file: &str) {
    println!(
        "Connection from: {}",
        stream
            .peer_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|_| "Unknown".to_string())
    );

    let mut buffer = [0; 4096];
    let bytes_read = match stream.read(&mut buffer) {
        Ok(0) => return, // Client closed connection
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to read from stream: {}", e);
            return;
        }
    };

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let mut lines = request.lines();

    // Parse the first request line
    let request_line = match lines.next() {
        Some(line) => line,
        None => {
            send_response(&mut stream, "400 Bad Request", None, "text/plain");
            return;
        }
    };

    let mut parts = request_line.split_whitespace();
    let method = parts.next();
    let path = parts.next();
    let http_version = parts.next();

    // Validate request structure
    if method != Some("GET") || path.is_none() || http_version != Some("HTTP/1.1") {
        send_response(&mut stream, "400 Bad Request", None, "text/plain");
        return;
    }

    let path = path.unwrap();
    println!("Requested path: {}", path);

    // Reject unsupported methods
    if method != Some("GET") {
        send_response(&mut stream, "405 Method Not Allowed", None, "text/plain");
        return;
    }

    // Validate and sanitize requested path
    match sanitize_path(&base_dir, path, index_file) {
        Some(file_path) => match fs::read(&file_path) {
            Ok(contents) => {
                // Use `mime_guess` to determine content type
                let content_type = from_path(&file_path).first_or_octet_stream().to_string();

                // Send response
                send_response(&mut stream, "200 OK", Some(&contents), &content_type);
                println!("Responded with 200 OK");
            }
            Err(_) => {
                send_response(&mut stream, "500 Internal Server Error", None, "text/plain");
                println!("Responded with 500 Internal Server Error");
            }
        },
        None => {
            send_response(&mut stream, "404 Not Found", None, "text/plain");
            println!("Responded with 404 Not Found");
        }
    }
}
