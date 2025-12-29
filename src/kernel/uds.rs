//! Unix Domain Sockets
//!
//! Provides local IPC using the filesystem namespace (AF_UNIX/AF_LOCAL).
//! Supports both stream (connection-oriented) and datagram (connectionless) sockets.

use std::collections::{HashMap, VecDeque};

/// Unix domain socket types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketType {
    /// Connection-oriented stream (like TCP)
    Stream,
    /// Connectionless datagram (like UDP)
    Datagram,
}

impl SocketType {
    /// Create from numeric type (matching POSIX SOCK_STREAM, SOCK_DGRAM)
    pub fn from_num(n: i32) -> Option<Self> {
        match n {
            1 => Some(SocketType::Stream),
            2 => Some(SocketType::Datagram),
            _ => None,
        }
    }

    /// Convert to numeric type
    pub fn to_num(self) -> i32 {
        match self {
            SocketType::Stream => 1,
            SocketType::Datagram => 2,
        }
    }
}

/// Socket state for stream sockets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    /// Created but not bound
    Unbound,
    /// Bound to an address
    Bound,
    /// Listening for connections (server)
    Listening,
    /// Connecting to peer
    Connecting,
    /// Connected to peer
    Connected,
    /// Connection closed
    Closed,
}

/// Unix domain socket address
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SockAddr {
    /// Path in the filesystem namespace
    pub path: String,
}

impl SockAddr {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// Check if this is an abstract socket (Linux extension)
    pub fn is_abstract(&self) -> bool {
        self.path.starts_with('\0')
    }

    /// Check if this is an unnamed socket
    pub fn is_unnamed(&self) -> bool {
        self.path.is_empty()
    }
}

/// Socket identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SocketId(pub u64);

/// Error types for socket operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketError {
    /// Socket not found
    NotFound,
    /// Address already in use
    AddressInUse,
    /// Connection refused
    ConnectionRefused,
    /// Socket not connected
    NotConnected,
    /// Socket already connected
    AlreadyConnected,
    /// Invalid operation for socket state
    InvalidState,
    /// Would block (non-blocking mode)
    WouldBlock,
    /// Connection reset by peer
    ConnectionReset,
    /// Buffer full
    BufferFull,
    /// Permission denied
    PermissionDenied,
    /// Operation not supported
    NotSupported,
}

impl std::fmt::Display for SocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SocketError::NotFound => write!(f, "socket not found"),
            SocketError::AddressInUse => write!(f, "address already in use"),
            SocketError::ConnectionRefused => write!(f, "connection refused"),
            SocketError::NotConnected => write!(f, "socket not connected"),
            SocketError::AlreadyConnected => write!(f, "socket already connected"),
            SocketError::InvalidState => write!(f, "invalid operation for socket state"),
            SocketError::WouldBlock => write!(f, "operation would block"),
            SocketError::ConnectionReset => write!(f, "connection reset by peer"),
            SocketError::BufferFull => write!(f, "buffer full"),
            SocketError::PermissionDenied => write!(f, "permission denied"),
            SocketError::NotSupported => write!(f, "operation not supported"),
        }
    }
}

impl std::error::Error for SocketError {}

/// Result type for socket operations
pub type SocketResult<T> = Result<T, SocketError>;

/// A Unix domain socket
#[derive(Debug)]
pub struct UnixSocket {
    /// Socket ID
    pub id: SocketId,
    /// Socket type
    pub socket_type: SocketType,
    /// Current state
    pub state: SocketState,
    /// Bound address (if any)
    pub local_addr: Option<SockAddr>,
    /// Peer address (if connected)
    pub peer_addr: Option<SockAddr>,
    /// Receive buffer
    recv_buffer: VecDeque<Vec<u8>>,
    /// Send buffer (for non-blocking)
    send_buffer: VecDeque<Vec<u8>>,
    /// Maximum buffer size
    buffer_size: usize,
    /// Non-blocking mode
    pub non_blocking: bool,
    /// Backlog for listening sockets
    backlog: usize,
    /// Pending connections (for listening sockets)
    pending_connections: VecDeque<SocketId>,
    /// Peer socket ID (for connected stream sockets)
    peer_socket: Option<SocketId>,
}

impl UnixSocket {
    /// Default buffer size (64KB)
    pub const DEFAULT_BUFFER_SIZE: usize = 65536;

    /// Create a new socket
    pub fn new(id: SocketId, socket_type: SocketType) -> Self {
        Self {
            id,
            socket_type,
            state: SocketState::Unbound,
            local_addr: None,
            peer_addr: None,
            recv_buffer: VecDeque::new(),
            send_buffer: VecDeque::new(),
            buffer_size: Self::DEFAULT_BUFFER_SIZE,
            non_blocking: false,
            backlog: 0,
            pending_connections: VecDeque::new(),
            peer_socket: None,
        }
    }

    /// Check if socket has data to read
    pub fn has_data(&self) -> bool {
        !self.recv_buffer.is_empty()
    }

    /// Check if socket can accept connections
    pub fn has_pending_connections(&self) -> bool {
        !self.pending_connections.is_empty()
    }

    /// Get receive buffer size
    pub fn recv_buffer_len(&self) -> usize {
        self.recv_buffer.iter().map(|v| v.len()).sum()
    }

    /// Get send buffer size
    pub fn send_buffer_len(&self) -> usize {
        self.send_buffer.iter().map(|v| v.len()).sum()
    }

    /// Push data to receive buffer
    pub fn push_recv(&mut self, data: Vec<u8>) -> SocketResult<()> {
        if self.recv_buffer_len() + data.len() > self.buffer_size {
            return Err(SocketError::BufferFull);
        }
        self.recv_buffer.push_back(data);
        Ok(())
    }

    /// Pop data from receive buffer
    pub fn pop_recv(&mut self) -> Option<Vec<u8>> {
        self.recv_buffer.pop_front()
    }

    /// Peek at receive buffer without removing
    pub fn peek_recv(&self) -> Option<&Vec<u8>> {
        self.recv_buffer.front()
    }

    /// Add pending connection
    pub fn add_pending_connection(&mut self, socket_id: SocketId) -> SocketResult<()> {
        if self.pending_connections.len() >= self.backlog {
            return Err(SocketError::BufferFull);
        }
        self.pending_connections.push_back(socket_id);
        Ok(())
    }

    /// Get next pending connection
    pub fn pop_pending_connection(&mut self) -> Option<SocketId> {
        self.pending_connections.pop_front()
    }
}

/// Unix domain socket manager
#[derive(Debug, Default)]
pub struct UnixSocketManager {
    /// All sockets by ID
    sockets: HashMap<SocketId, UnixSocket>,
    /// Bound addresses to socket IDs
    bound_addresses: HashMap<String, SocketId>,
    /// Next socket ID
    next_id: u64,
}

impl UnixSocketManager {
    /// Create a new socket manager
    pub fn new() -> Self {
        Self {
            sockets: HashMap::new(),
            bound_addresses: HashMap::new(),
            next_id: 1,
        }
    }

    /// Create a new socket
    pub fn socket(&mut self, socket_type: SocketType) -> SocketId {
        let id = SocketId(self.next_id);
        self.next_id += 1;
        let socket = UnixSocket::new(id, socket_type);
        self.sockets.insert(id, socket);
        id
    }

    /// Close and remove a socket
    pub fn close(&mut self, id: SocketId) -> SocketResult<()> {
        if let Some(socket) = self.sockets.remove(&id) {
            // Remove from bound addresses
            if let Some(addr) = &socket.local_addr {
                self.bound_addresses.remove(&addr.path);
            }
            Ok(())
        } else {
            Err(SocketError::NotFound)
        }
    }

    /// Bind a socket to an address
    pub fn bind(&mut self, id: SocketId, addr: SockAddr) -> SocketResult<()> {
        // Check if address is already in use
        if self.bound_addresses.contains_key(&addr.path) {
            return Err(SocketError::AddressInUse);
        }

        let socket = self.sockets.get_mut(&id).ok_or(SocketError::NotFound)?;

        // Check state
        if socket.state != SocketState::Unbound {
            return Err(SocketError::InvalidState);
        }

        // Bind the socket
        socket.local_addr = Some(addr.clone());
        socket.state = SocketState::Bound;
        self.bound_addresses.insert(addr.path, id);

        Ok(())
    }

    /// Listen for connections (stream sockets only)
    pub fn listen(&mut self, id: SocketId, backlog: usize) -> SocketResult<()> {
        let socket = self.sockets.get_mut(&id).ok_or(SocketError::NotFound)?;

        // Check type
        if socket.socket_type != SocketType::Stream {
            return Err(SocketError::NotSupported);
        }

        // Check state
        if socket.state != SocketState::Bound {
            return Err(SocketError::InvalidState);
        }

        socket.state = SocketState::Listening;
        socket.backlog = backlog.max(1);

        Ok(())
    }

    /// Accept a connection (stream sockets only)
    pub fn accept(&mut self, id: SocketId) -> SocketResult<(SocketId, SockAddr)> {
        // First, validate and get information we need from the listening socket
        let (client_id, server_local_addr) = {
            let socket = self.sockets.get_mut(&id).ok_or(SocketError::NotFound)?;

            // Check type and state
            if socket.socket_type != SocketType::Stream {
                return Err(SocketError::NotSupported);
            }
            if socket.state != SocketState::Listening {
                return Err(SocketError::InvalidState);
            }

            // Get pending connection
            let client_id = socket
                .pop_pending_connection()
                .ok_or(SocketError::WouldBlock)?;

            (client_id, socket.local_addr.clone())
        };

        // Get client socket's address
        let client_addr = self
            .sockets
            .get(&client_id)
            .and_then(|s| s.local_addr.clone())
            .unwrap_or_else(|| SockAddr::new(""));

        // Create server-side socket for this connection
        let server_socket_id = self.socket(SocketType::Stream);

        // Set up the connection on server socket
        if let Some(server_socket) = self.sockets.get_mut(&server_socket_id) {
            server_socket.state = SocketState::Connected;
            server_socket.peer_addr = Some(client_addr.clone());
            server_socket.peer_socket = Some(client_id);
            server_socket.local_addr = server_local_addr;
        }

        // Update client socket
        if let Some(client_socket) = self.sockets.get_mut(&client_id) {
            client_socket.state = SocketState::Connected;
            client_socket.peer_socket = Some(server_socket_id);
        }

        Ok((server_socket_id, client_addr))
    }

    /// Connect to a listening socket (stream sockets only)
    pub fn connect(&mut self, id: SocketId, addr: &SockAddr) -> SocketResult<()> {
        // Find the listening socket
        let server_id = self
            .bound_addresses
            .get(&addr.path)
            .copied()
            .ok_or(SocketError::ConnectionRefused)?;

        // Check server socket state
        let server_socket = self.sockets.get(&server_id).ok_or(SocketError::NotFound)?;
        if server_socket.state != SocketState::Listening {
            return Err(SocketError::ConnectionRefused);
        }

        // Check client socket state
        let client_socket = self.sockets.get(&id).ok_or(SocketError::NotFound)?;
        if client_socket.socket_type != SocketType::Stream {
            return Err(SocketError::NotSupported);
        }
        if client_socket.state != SocketState::Unbound && client_socket.state != SocketState::Bound
        {
            return Err(SocketError::InvalidState);
        }

        // Add to server's pending connections
        let server_socket = self
            .sockets
            .get_mut(&server_id)
            .ok_or(SocketError::NotFound)?;
        server_socket.add_pending_connection(id)?;

        // Update client socket state
        let client_socket = self.sockets.get_mut(&id).ok_or(SocketError::NotFound)?;
        client_socket.peer_addr = Some(addr.clone());
        client_socket.state = SocketState::Connecting;

        Ok(())
    }

    /// Send data on a connected socket
    pub fn send(&mut self, id: SocketId, data: &[u8]) -> SocketResult<usize> {
        let socket = self.sockets.get(&id).ok_or(SocketError::NotFound)?;

        // Check state
        if socket.state != SocketState::Connected {
            return Err(SocketError::NotConnected);
        }

        let peer_id = socket.peer_socket.ok_or(SocketError::NotConnected)?;

        // Push to peer's receive buffer
        let peer = self
            .sockets
            .get_mut(&peer_id)
            .ok_or(SocketError::NotConnected)?;
        peer.push_recv(data.to_vec())?;

        Ok(data.len())
    }

    /// Receive data from a connected socket
    pub fn recv(&mut self, id: SocketId) -> SocketResult<Vec<u8>> {
        let socket = self.sockets.get_mut(&id).ok_or(SocketError::NotFound)?;

        // Check state
        if socket.state != SocketState::Connected {
            return Err(SocketError::NotConnected);
        }

        // Pop from receive buffer
        socket.pop_recv().ok_or(SocketError::WouldBlock)
    }

    /// Send datagram to address (datagram sockets only)
    pub fn sendto(&mut self, id: SocketId, data: &[u8], addr: &SockAddr) -> SocketResult<usize> {
        let socket = self.sockets.get(&id).ok_or(SocketError::NotFound)?;

        // Check type
        if socket.socket_type != SocketType::Datagram {
            return Err(SocketError::NotSupported);
        }

        // Find target socket
        let target_id = self
            .bound_addresses
            .get(&addr.path)
            .copied()
            .ok_or(SocketError::ConnectionRefused)?;

        // Push to target's receive buffer
        let target = self
            .sockets
            .get_mut(&target_id)
            .ok_or(SocketError::NotFound)?;
        target.push_recv(data.to_vec())?;

        Ok(data.len())
    }

    /// Receive datagram (datagram sockets only)
    pub fn recvfrom(&mut self, id: SocketId) -> SocketResult<(Vec<u8>, Option<SockAddr>)> {
        let socket = self.sockets.get_mut(&id).ok_or(SocketError::NotFound)?;

        // Check type
        if socket.socket_type != SocketType::Datagram {
            return Err(SocketError::NotSupported);
        }

        // Pop from receive buffer
        let data = socket.pop_recv().ok_or(SocketError::WouldBlock)?;

        // Note: We don't track sender addresses for now
        Ok((data, None))
    }

    /// Get socket by ID
    pub fn get(&self, id: SocketId) -> Option<&UnixSocket> {
        self.sockets.get(&id)
    }

    /// Get mutable socket by ID
    pub fn get_mut(&mut self, id: SocketId) -> Option<&mut UnixSocket> {
        self.sockets.get_mut(&id)
    }

    /// Check if socket has data
    pub fn has_data(&self, id: SocketId) -> bool {
        self.sockets.get(&id).is_some_and(|s| s.has_data())
    }

    /// Check if listening socket has pending connections
    pub fn has_pending(&self, id: SocketId) -> bool {
        self.sockets
            .get(&id)
            .is_some_and(|s| s.has_pending_connections())
    }

    /// Get socket state
    pub fn state(&self, id: SocketId) -> Option<SocketState> {
        self.sockets.get(&id).map(|s| s.state)
    }

    /// Set non-blocking mode
    pub fn set_nonblocking(&mut self, id: SocketId, nonblocking: bool) -> SocketResult<()> {
        let socket = self.sockets.get_mut(&id).ok_or(SocketError::NotFound)?;
        socket.non_blocking = nonblocking;
        Ok(())
    }

    /// Get local address
    pub fn local_addr(&self, id: SocketId) -> SocketResult<Option<SockAddr>> {
        let socket = self.sockets.get(&id).ok_or(SocketError::NotFound)?;
        Ok(socket.local_addr.clone())
    }

    /// Get peer address
    pub fn peer_addr(&self, id: SocketId) -> SocketResult<Option<SockAddr>> {
        let socket = self.sockets.get(&id).ok_or(SocketError::NotFound)?;
        Ok(socket.peer_addr.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_type_conversion() {
        assert_eq!(SocketType::from_num(1), Some(SocketType::Stream));
        assert_eq!(SocketType::from_num(2), Some(SocketType::Datagram));
        assert_eq!(SocketType::from_num(0), None);

        assert_eq!(SocketType::Stream.to_num(), 1);
        assert_eq!(SocketType::Datagram.to_num(), 2);
    }

    #[test]
    fn test_socket_creation() {
        let mut mgr = UnixSocketManager::new();
        let id = mgr.socket(SocketType::Stream);
        assert!(mgr.get(id).is_some());
        assert_eq!(mgr.state(id), Some(SocketState::Unbound));
    }

    #[test]
    fn test_socket_bind() {
        let mut mgr = UnixSocketManager::new();
        let id = mgr.socket(SocketType::Stream);

        let addr = SockAddr::new("/tmp/test.sock");
        assert!(mgr.bind(id, addr.clone()).is_ok());
        assert_eq!(mgr.state(id), Some(SocketState::Bound));
        assert_eq!(mgr.local_addr(id).unwrap(), Some(addr));
    }

    #[test]
    fn test_bind_address_in_use() {
        let mut mgr = UnixSocketManager::new();
        let id1 = mgr.socket(SocketType::Stream);
        let id2 = mgr.socket(SocketType::Stream);

        let addr = SockAddr::new("/tmp/conflict.sock");
        assert!(mgr.bind(id1, addr.clone()).is_ok());
        assert_eq!(mgr.bind(id2, addr), Err(SocketError::AddressInUse));
    }

    #[test]
    fn test_socket_listen() {
        let mut mgr = UnixSocketManager::new();
        let id = mgr.socket(SocketType::Stream);
        mgr.bind(id, SockAddr::new("/tmp/server.sock")).unwrap();
        assert!(mgr.listen(id, 5).is_ok());
        assert_eq!(mgr.state(id), Some(SocketState::Listening));
    }

    #[test]
    fn test_stream_connect_accept() {
        let mut mgr = UnixSocketManager::new();

        // Create and set up server
        let server_id = mgr.socket(SocketType::Stream);
        let server_addr = SockAddr::new("/tmp/server.sock");
        mgr.bind(server_id, server_addr.clone()).unwrap();
        mgr.listen(server_id, 5).unwrap();

        // Create client and connect
        let client_id = mgr.socket(SocketType::Stream);
        assert!(mgr.connect(client_id, &server_addr).is_ok());
        assert_eq!(mgr.state(client_id), Some(SocketState::Connecting));

        // Accept on server
        let (accepted_id, _) = mgr.accept(server_id).unwrap();
        assert_eq!(mgr.state(accepted_id), Some(SocketState::Connected));
        assert_eq!(mgr.state(client_id), Some(SocketState::Connected));
    }

    #[test]
    fn test_stream_send_recv() {
        let mut mgr = UnixSocketManager::new();

        // Set up connected sockets
        let server_id = mgr.socket(SocketType::Stream);
        let server_addr = SockAddr::new("/tmp/echo.sock");
        mgr.bind(server_id, server_addr.clone()).unwrap();
        mgr.listen(server_id, 5).unwrap();

        let client_id = mgr.socket(SocketType::Stream);
        mgr.connect(client_id, &server_addr).unwrap();
        let (accepted_id, _) = mgr.accept(server_id).unwrap();

        // Send from client
        let data = b"hello server";
        assert_eq!(mgr.send(client_id, data).unwrap(), data.len());

        // Receive on server
        let received = mgr.recv(accepted_id).unwrap();
        assert_eq!(received, data);

        // Send from server
        let response = b"hello client";
        assert_eq!(mgr.send(accepted_id, response).unwrap(), response.len());

        // Receive on client
        let received = mgr.recv(client_id).unwrap();
        assert_eq!(received, response);
    }

    #[test]
    fn test_datagram_sendto_recvfrom() {
        let mut mgr = UnixSocketManager::new();

        // Create and bind two datagram sockets
        let sock1 = mgr.socket(SocketType::Datagram);
        let addr1 = SockAddr::new("/tmp/dgram1.sock");
        mgr.bind(sock1, addr1.clone()).unwrap();

        let sock2 = mgr.socket(SocketType::Datagram);
        let addr2 = SockAddr::new("/tmp/dgram2.sock");
        mgr.bind(sock2, addr2.clone()).unwrap();

        // Send datagram from sock1 to sock2
        let data = b"datagram message";
        assert_eq!(mgr.sendto(sock1, data, &addr2).unwrap(), data.len());

        // Receive on sock2
        let (received, _) = mgr.recvfrom(sock2).unwrap();
        assert_eq!(received, data);
    }

    #[test]
    fn test_socket_close() {
        let mut mgr = UnixSocketManager::new();
        let id = mgr.socket(SocketType::Stream);
        let addr = SockAddr::new("/tmp/close.sock");
        mgr.bind(id, addr.clone()).unwrap();

        assert!(mgr.close(id).is_ok());
        assert!(mgr.get(id).is_none());

        // Address should be available again
        let id2 = mgr.socket(SocketType::Stream);
        assert!(mgr.bind(id2, addr).is_ok());
    }

    #[test]
    fn test_recv_would_block() {
        let mut mgr = UnixSocketManager::new();

        // Set up connected sockets
        let server_id = mgr.socket(SocketType::Stream);
        let server_addr = SockAddr::new("/tmp/block.sock");
        mgr.bind(server_id, server_addr.clone()).unwrap();
        mgr.listen(server_id, 5).unwrap();

        let client_id = mgr.socket(SocketType::Stream);
        mgr.connect(client_id, &server_addr).unwrap();
        let (accepted_id, _) = mgr.accept(server_id).unwrap();

        // Try to receive without data
        assert_eq!(mgr.recv(accepted_id), Err(SocketError::WouldBlock));
    }

    #[test]
    fn test_connect_refused() {
        let mut mgr = UnixSocketManager::new();

        let client_id = mgr.socket(SocketType::Stream);
        let addr = SockAddr::new("/tmp/nonexistent.sock");

        assert_eq!(
            mgr.connect(client_id, &addr),
            Err(SocketError::ConnectionRefused)
        );
    }

    #[test]
    fn test_nonblocking_mode() {
        let mut mgr = UnixSocketManager::new();
        let id = mgr.socket(SocketType::Stream);

        assert!(mgr.set_nonblocking(id, true).is_ok());
        assert!(mgr.get(id).unwrap().non_blocking);

        assert!(mgr.set_nonblocking(id, false).is_ok());
        assert!(!mgr.get(id).unwrap().non_blocking);
    }

    #[test]
    fn test_sockaddr_types() {
        let regular = SockAddr::new("/tmp/regular.sock");
        assert!(!regular.is_abstract());
        assert!(!regular.is_unnamed());

        let abstract_sock = SockAddr::new("\0abstract");
        assert!(abstract_sock.is_abstract());

        let unnamed = SockAddr::new("");
        assert!(unnamed.is_unnamed());
    }
}
