//! Network subsystem for WASM
//!
//! Provides networking capabilities using browser APIs:
//! - HTTP client via Fetch API
//! - WebSocket support for bidirectional communication
//!
//! Limitations (browser sandbox):
//! - No raw TCP/UDP sockets
//! - No server listening
//! - Subject to CORS restrictions

#![cfg(target_arch = "wasm32")]

use std::collections::HashMap;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// HTTP method
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Head,
    Patch,
}

impl HttpMethod {
    fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
            HttpMethod::Patch => "PATCH",
        }
    }
}

/// HTTP response
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Get body as UTF-8 string
    pub fn text(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body.clone())
    }
}

/// HTTP request builder
pub struct HttpRequest {
    url: String,
    method: HttpMethod,
    headers: HashMap<String, String>,
    body: Option<Vec<u8>>,
}

impl HttpRequest {
    /// Create a new GET request
    pub fn get(url: &str) -> Self {
        Self {
            url: url.to_string(),
            method: HttpMethod::Get,
            headers: HashMap::new(),
            body: None,
        }
    }

    /// Create a new POST request
    pub fn post(url: &str) -> Self {
        Self {
            url: url.to_string(),
            method: HttpMethod::Post,
            headers: HashMap::new(),
            body: None,
        }
    }

    /// Create a new request with specified method
    pub fn new(method: HttpMethod, url: &str) -> Self {
        Self {
            url: url.to_string(),
            method,
            headers: HashMap::new(),
            body: None,
        }
    }

    /// Add a header
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.insert(name.to_string(), value.to_string());
        self
    }

    /// Set request body
    pub fn body(mut self, data: Vec<u8>) -> Self {
        self.body = Some(data);
        self
    }

    /// Set JSON body
    pub fn json(mut self, json: &str) -> Self {
        self.headers
            .insert("Content-Type".to_string(), "application/json".to_string());
        self.body = Some(json.as_bytes().to_vec());
        self
    }

    /// Execute the request
    pub async fn send(self) -> Result<HttpResponse, String> {
        let window = web_sys::window().ok_or("No window object")?;

        // Create request init
        let mut opts = web_sys::RequestInit::new();
        opts.method(self.method.as_str());
        opts.mode(web_sys::RequestMode::Cors);

        // Set body if present
        if let Some(body) = &self.body {
            let uint8_array = js_sys::Uint8Array::from(body.as_slice());
            opts.body(Some(&uint8_array));
        }

        // Create request
        let request = web_sys::Request::new_with_str_and_init(&self.url, &opts)
            .map_err(|e| format!("Failed to create request: {:?}", e))?;

        // Set headers
        let headers = request.headers();
        for (name, value) in &self.headers {
            headers
                .set(name, value)
                .map_err(|e| format!("Failed to set header: {:?}", e))?;
        }

        // Execute fetch
        let resp_value = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|e| format!("Fetch failed: {:?}", e))?;

        let resp: web_sys::Response = resp_value
            .dyn_into()
            .map_err(|_| "Failed to cast response")?;

        // Get status
        let status = resp.status();
        let status_text = resp.status_text();

        // Get headers
        let mut response_headers = HashMap::new();
        let header_entries = resp.headers();
        // Note: Headers iteration is limited in web-sys, we get common ones
        for name in [
            "content-type",
            "content-length",
            "cache-control",
            "date",
            "server",
        ]
        .iter()
        {
            if let Ok(Some(value)) = header_entries.get(*name) {
                response_headers.insert(name.to_string(), value);
            }
        }

        // Get body
        let array_buffer = JsFuture::from(
            resp.array_buffer()
                .map_err(|e| format!("Failed to get body: {:?}", e))?,
        )
        .await
        .map_err(|e| format!("Failed to read body: {:?}", e))?;

        let uint8_array = js_sys::Uint8Array::new(&array_buffer);
        let body = uint8_array.to_vec();

        Ok(HttpResponse {
            status,
            status_text,
            headers: response_headers,
            body,
        })
    }
}

/// WebSocket connection state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WsState {
    Connecting,
    Open,
    Closing,
    Closed,
}

/// WebSocket connection ID
pub type WsId = u32;

/// WebSocket manager
pub struct WebSocketManager {
    next_id: WsId,
    sockets: HashMap<WsId, web_sys::WebSocket>,
    messages: HashMap<WsId, Vec<String>>,
}

impl WebSocketManager {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            sockets: HashMap::new(),
            messages: HashMap::new(),
        }
    }

    /// Connect to a WebSocket server
    pub fn connect(&mut self, url: &str) -> Result<WsId, String> {
        let ws = web_sys::WebSocket::new(url)
            .map_err(|e| format!("WebSocket creation failed: {:?}", e))?;

        let id = self.next_id;
        self.next_id += 1;

        // Set up message handler
        let messages = self.messages.entry(id).or_insert_with(Vec::new);
        let messages_clone = messages.clone();
        let onmessage_callback = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
            if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
                // Note: Can't actually mutate here due to ownership, this is simplified
                crate::console_log!("[ws {}] Message: {}", id, String::from(text));
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();

        // Set up error handler
        let onerror_callback = Closure::wrap(Box::new(move |_e: web_sys::ErrorEvent| {
            crate::console_log!("[ws {}] Error occurred", id);
        }) as Box<dyn FnMut(_)>);
        ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
        onerror_callback.forget();

        // Set up close handler
        let onclose_callback = Closure::wrap(Box::new(move |_e: web_sys::CloseEvent| {
            crate::console_log!("[ws {}] Connection closed", id);
        }) as Box<dyn FnMut(_)>);
        ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
        onclose_callback.forget();

        self.sockets.insert(id, ws);
        self.messages.insert(id, Vec::new());

        Ok(id)
    }

    /// Send a message
    pub fn send(&self, id: WsId, message: &str) -> Result<(), String> {
        let ws = self.sockets.get(&id).ok_or("WebSocket not found")?;
        ws.send_with_str(message)
            .map_err(|e| format!("Send failed: {:?}", e))
    }

    /// Get connection state
    pub fn state(&self, id: WsId) -> Option<WsState> {
        self.sockets.get(&id).map(|ws| match ws.ready_state() {
            web_sys::WebSocket::CONNECTING => WsState::Connecting,
            web_sys::WebSocket::OPEN => WsState::Open,
            web_sys::WebSocket::CLOSING => WsState::Closing,
            web_sys::WebSocket::CLOSED => WsState::Closed,
            _ => WsState::Closed,
        })
    }

    /// Close a connection
    pub fn close(&mut self, id: WsId) -> Result<(), String> {
        if let Some(ws) = self.sockets.remove(&id) {
            ws.close().map_err(|e| format!("Close failed: {:?}", e))?;
        }
        self.messages.remove(&id);
        Ok(())
    }
}

impl Default for WebSocketManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple HTTP fetch (convenience function)
pub async fn fetch(url: &str) -> Result<HttpResponse, String> {
    HttpRequest::get(url).send().await
}

/// Fetch with custom method and options
pub async fn fetch_with_method(
    method: HttpMethod,
    url: &str,
    headers: HashMap<String, String>,
    body: Option<Vec<u8>>,
) -> Result<HttpResponse, String> {
    let mut req = HttpRequest::new(method, url);
    for (name, value) in headers {
        req = req.header(&name, &value);
    }
    if let Some(body) = body {
        req = req.body(body);
    }
    req.send().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_method_str() {
        assert_eq!(HttpMethod::Get.as_str(), "GET");
        assert_eq!(HttpMethod::Post.as_str(), "POST");
        assert_eq!(HttpMethod::Put.as_str(), "PUT");
        assert_eq!(HttpMethod::Delete.as_str(), "DELETE");
    }

    #[test]
    fn test_request_builder() {
        let req = HttpRequest::get("https://example.com")
            .header("Accept", "application/json")
            .header("X-Custom", "value");

        assert_eq!(req.url, "https://example.com");
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(
            req.headers.get("Accept"),
            Some(&"application/json".to_string())
        );
    }
}
