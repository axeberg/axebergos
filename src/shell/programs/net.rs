//! Network programs module
//!
//! This module contains network-related shell programs for HTTP requests and file downloads.
//! These programs are primarily functional in WASM builds where browser APIs are available.
//!
//! Programs:
//! - `curl`: Transfer data from URLs with support for custom methods and headers
//! - `wget`: Download files from URLs to the filesystem

use super::{args_to_strs, check_help};
#[cfg(target_arch = "wasm32")]
use crate::kernel::syscall;

/// curl - transfer data from URL
pub fn prog_curl(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);
    if let Some(help) = check_help(&args, "Usage: curl [OPTIONS] URL\nTransfer data from URL.\n  -i  Include headers in output\n  -s  Silent mode\n  -X METHOD  Specify request method\n  -H HEADER  Add custom header\nSee 'man curl' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    // Parse URL from arguments (needed for both wasm and non-wasm paths)
    let url: String = args.iter()
        .find(|s| !s.starts_with('-') && !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_default();

    if url.is_empty() {
        stderr.push_str("curl: no URL specified\n");
        return 1;
    }

    #[cfg(target_arch = "wasm32")]
    {
        use crate::kernel::network::{HttpMethod, HttpRequest};

        // Parse WASM-specific options
        let mut include_headers = false;
        let mut method = "GET";
        let mut headers: Vec<(String, String)> = Vec::new();
        let mut i = 0;

        while i < args.len() {
            match args[i] {
                "-i" => include_headers = true,
                "-s" => {} // silent mode
                "-X" => {
                    i += 1;
                    if i < args.len() {
                        method = args[i];
                    }
                }
                "-H" => {
                    i += 1;
                    if i < args.len() {
                        if let Some(pos) = args[i].find(':') {
                            let name = args[i][..pos].trim().to_string();
                            let value = args[i][pos+1..].trim().to_string();
                            headers.push((name, value));
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }

        let http_method = match method.to_uppercase().as_str() {
            "GET" => HttpMethod::Get,
            "POST" => HttpMethod::Post,
            "PUT" => HttpMethod::Put,
            "DELETE" => HttpMethod::Delete,
            "HEAD" => HttpMethod::Head,
            "PATCH" => HttpMethod::Patch,
            _ => {
                stderr.push_str(&format!("curl: unsupported method: {}\n", method));
                return 1;
            }
        };

        let url_clone = url.clone();
        let include_headers_clone = include_headers;
        let headers_clone = headers.clone();

        wasm_bindgen_futures::spawn_local(async move {
            let mut req = HttpRequest::new(http_method, &url_clone);
            for (name, value) in headers_clone {
                req = req.header(&name, &value);
            }

            match req.send().await {
                Ok(resp) => {
                    if include_headers_clone {
                        crate::console_log!("HTTP/{} {}", resp.status, resp.status_text);
                        for (name, value) in &resp.headers {
                            crate::console_log!("{}: {}", name, value);
                        }
                        crate::console_log!("");
                    }
                    match resp.text() {
                        Ok(text) => crate::console_log!("{}", text),
                        Err(_) => crate::console_log!("[binary data: {} bytes]", resp.body.len()),
                    }
                }
                Err(e) => {
                    crate::console_log!("curl: {}", e);
                }
            }
        });
        stdout.push_str(&format!("Fetching {}...\n", url));
        stdout.push_str("(Check browser console for result)\n");
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        stdout.push_str("curl: not available in this build (requires WASM)\n");
    }

    0
}

/// wget - download file from URL
#[allow(unused_variables)]
pub fn prog_wget(args: &[String], stdin: &str, stdout: &mut String, stderr: &mut String) -> i32 {
    let args = args_to_strs(args);
    if let Some(help) = check_help(&args, "Usage: wget [OPTIONS] URL\nDownload file from URL.\n  -O FILE  Save to FILE instead of default\n  -q       Quiet mode\nSee 'man wget' for details.") {
        stdout.push_str(&help);
        return 0;
    }

    // Parse arguments
    let mut url = String::new();
    let mut output_file = String::new();
    let mut i = 0;

    while i < args.len() {
        match args[i] {
            "-q" => {} // quiet mode
            "-O" => {
                i += 1;
                if i < args.len() {
                    output_file = args[i].to_string();
                }
            }
            s if !s.starts_with('-') => {
                url = s.to_string();
            }
            _ => {}
        }
        i += 1;
    }

    if url.is_empty() {
        stderr.push_str("wget: no URL specified\n");
        return 1;
    }

    // Determine output filename
    let filename = if output_file.is_empty() {
        // Extract filename from URL
        url.rsplit('/').next().unwrap_or("index.html").to_string()
    } else {
        output_file
    };

    #[cfg(target_arch = "wasm32")]
    {
        use crate::kernel::network::HttpRequest;

        let url_clone = url.clone();
        let filename_clone = filename.clone();

        wasm_bindgen_futures::spawn_local(async move {
            match HttpRequest::get(&url_clone).send().await {
                Ok(resp) => {
                    if resp.status >= 200 && resp.status < 300 {
                        // Write to file
                        match syscall::write_file(&filename_clone, &String::from_utf8_lossy(&resp.body)) {
                            Ok(_) => {
                                crate::console_log!("Downloaded {} -> {} ({} bytes)",
                                    url_clone, filename_clone, resp.body.len());
                            }
                            Err(e) => {
                                crate::console_log!("wget: failed to write {}: {}", filename_clone, e);
                            }
                        }
                    } else {
                        crate::console_log!("wget: HTTP {} {}", resp.status, resp.status_text);
                    }
                }
                Err(e) => {
                    crate::console_log!("wget: {}", e);
                }
            }
        });
        stdout.push_str(&format!("Downloading {} -> {}...\n", url, filename));
        stdout.push_str("(Check browser console for result)\n");
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        stdout.push_str("wget: not available in this build (requires WASM)\n");
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_curl_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_curl(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: curl"));
        assert!(stdout.contains("-i"));
        assert!(stdout.contains("-X METHOD"));
        assert!(stdout.contains("-H HEADER"));
    }

    #[test]
    fn test_curl_help_short() {
        let args = vec!["-h".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_curl(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: curl"));
    }

    #[test]
    fn test_curl_no_url() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_curl(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("no URL specified"));
    }

    #[test]
    fn test_wget_help() {
        let args = vec!["--help".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_wget(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: wget"));
        assert!(stdout.contains("-O FILE"));
        assert!(stdout.contains("-q"));
    }

    #[test]
    fn test_wget_help_short() {
        let args = vec!["-h".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_wget(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        assert!(stdout.contains("Usage: wget"));
    }

    #[test]
    fn test_wget_no_url() {
        let args = vec![];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_wget(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 1);
        assert!(stderr.contains("no URL specified"));
    }

    #[test]
    fn test_wget_non_wasm() {
        // In non-WASM builds, wget outputs a "not available" message
        let args = vec!["http://example.com".to_string()];
        let mut stdout = String::new();
        let mut stderr = String::new();
        let result = prog_wget(&args, "", &mut stdout, &mut stderr);
        assert_eq!(result, 0);
        // Non-WASM build returns a message about WASM requirement
        assert!(stdout.contains("not available") || stdout.contains("Downloading"));
    }
}
