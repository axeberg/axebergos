//! Message queue implementation
//!
//! System V-style message queues for inter-process communication.
//! Messages are tagged with a type for selective receiving.

use std::collections::{HashMap, VecDeque};

/// A message in the queue
#[derive(Debug, Clone)]
pub struct Message {
    /// Message type (must be > 0)
    pub mtype: i64,
    /// Message data
    pub data: Vec<u8>,
}

impl Message {
    pub fn new(mtype: i64, data: Vec<u8>) -> Self {
        Self { mtype, data }
    }
}

/// Message queue
#[derive(Debug)]
pub struct MessageQueue {
    /// Queue ID
    pub id: MsgQueueId,
    /// Messages in the queue
    messages: VecDeque<Message>,
    /// Maximum number of bytes in queue
    max_bytes: usize,
    /// Current bytes used
    current_bytes: usize,
    /// Owner UID
    pub uid: u32,
    /// Owner GID
    pub gid: u32,
    /// Permissions mode
    pub mode: u16,
    /// Last send time
    pub stime: f64,
    /// Last receive time
    pub rtime: f64,
    /// Number of messages sent
    pub msg_snd: u64,
    /// Number of messages received
    pub msg_rcv: u64,
}

impl MessageQueue {
    pub fn new(id: MsgQueueId, uid: u32, gid: u32) -> Self {
        Self {
            id,
            messages: VecDeque::new(),
            max_bytes: 16384, // 16KB default
            current_bytes: 0,
            uid,
            gid,
            mode: 0o644,
            stime: 0.0,
            rtime: 0.0,
            msg_snd: 0,
            msg_rcv: 0,
        }
    }

    /// Send a message to the queue
    pub fn send(&mut self, msg: Message, now: f64) -> Result<(), MsgQueueError> {
        if msg.mtype <= 0 {
            return Err(MsgQueueError::InvalidType);
        }

        let msg_size = msg.data.len();
        if self.current_bytes + msg_size > self.max_bytes {
            return Err(MsgQueueError::QueueFull);
        }

        self.current_bytes += msg_size;
        self.messages.push_back(msg);
        self.stime = now;
        self.msg_snd += 1;
        Ok(())
    }

    /// Receive a message from the queue
    ///
    /// - mtype == 0: receive first message
    /// - mtype > 0: receive first message with matching type
    /// - mtype < 0: receive first message with type <= |mtype|
    pub fn receive(&mut self, mtype: i64, now: f64) -> Result<Message, MsgQueueError> {
        let idx = if mtype == 0 {
            // Any message
            if self.messages.is_empty() {
                return Err(MsgQueueError::NoMessage);
            }
            Some(0)
        } else if mtype > 0 {
            // Exact type match
            self.messages.iter().position(|m| m.mtype == mtype)
        } else {
            // First message with type <= |mtype|
            let abs_type = mtype.abs();
            self.messages.iter().position(|m| m.mtype <= abs_type)
        };

        match idx {
            Some(i) => {
                let msg = self.messages.remove(i).unwrap();
                self.current_bytes -= msg.data.len();
                self.rtime = now;
                self.msg_rcv += 1;
                Ok(msg)
            }
            None => Err(MsgQueueError::NoMessage),
        }
    }

    /// Peek at messages without removing
    pub fn peek(&self, mtype: i64) -> Option<&Message> {
        if mtype == 0 {
            self.messages.front()
        } else if mtype > 0 {
            self.messages.iter().find(|m| m.mtype == mtype)
        } else {
            let abs_type = mtype.abs();
            self.messages.iter().find(|m| m.mtype <= abs_type)
        }
    }

    /// Get queue stats
    pub fn stats(&self) -> MsgQueueStats {
        MsgQueueStats {
            msg_qnum: self.messages.len(),
            msg_qbytes: self.max_bytes,
            msg_cbytes: self.current_bytes,
            msg_snd: self.msg_snd,
            msg_rcv: self.msg_rcv,
            stime: self.stime,
            rtime: self.rtime,
        }
    }

    /// Set maximum bytes
    pub fn set_max_bytes(&mut self, max: usize) {
        self.max_bytes = max;
    }
}

/// Message queue ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MsgQueueId(pub u32);

/// Message queue error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsgQueueError {
    /// Invalid message type (must be > 0)
    InvalidType,
    /// Queue is full
    QueueFull,
    /// No matching message
    NoMessage,
    /// Queue not found
    NotFound,
    /// Permission denied
    PermissionDenied,
    /// Already exists
    AlreadyExists,
}

/// Message queue statistics
#[derive(Debug, Clone)]
pub struct MsgQueueStats {
    /// Number of messages in queue
    pub msg_qnum: usize,
    /// Max bytes allowed
    pub msg_qbytes: usize,
    /// Current bytes used
    pub msg_cbytes: usize,
    /// Total messages sent
    pub msg_snd: u64,
    /// Total messages received
    pub msg_rcv: u64,
    /// Last send time
    pub stime: f64,
    /// Last receive time
    pub rtime: f64,
}

/// Message queue manager
pub struct MsgQueueManager {
    /// All message queues
    queues: HashMap<MsgQueueId, MessageQueue>,
    /// Key to ID mapping
    key_map: HashMap<i32, MsgQueueId>,
    /// Next queue ID
    next_id: u32,
}

impl MsgQueueManager {
    pub fn new() -> Self {
        Self {
            queues: HashMap::new(),
            key_map: HashMap::new(),
            next_id: 1,
        }
    }

    /// Get or create a message queue
    ///
    /// key < 0: create private queue
    /// key >= 0: get existing or create new
    pub fn msgget(&mut self, key: i32, uid: u32, gid: u32, create: bool) -> Result<MsgQueueId, MsgQueueError> {
        if key < 0 {
            // Private queue
            let id = MsgQueueId(self.next_id);
            self.next_id += 1;
            let queue = MessageQueue::new(id, uid, gid);
            self.queues.insert(id, queue);
            return Ok(id);
        }

        // Check if exists
        if let Some(&id) = self.key_map.get(&key) {
            return Ok(id);
        }

        if !create {
            return Err(MsgQueueError::NotFound);
        }

        // Create new
        let id = MsgQueueId(self.next_id);
        self.next_id += 1;
        let queue = MessageQueue::new(id, uid, gid);
        self.queues.insert(id, queue);
        self.key_map.insert(key, id);
        Ok(id)
    }

    /// Send a message
    pub fn msgsnd(&mut self, id: MsgQueueId, msg: Message, now: f64) -> Result<(), MsgQueueError> {
        let queue = self.queues.get_mut(&id).ok_or(MsgQueueError::NotFound)?;
        queue.send(msg, now)
    }

    /// Receive a message
    pub fn msgrcv(&mut self, id: MsgQueueId, mtype: i64, now: f64) -> Result<Message, MsgQueueError> {
        let queue = self.queues.get_mut(&id).ok_or(MsgQueueError::NotFound)?;
        queue.receive(mtype, now)
    }

    /// Get queue stats
    pub fn msgctl_stat(&self, id: MsgQueueId) -> Result<MsgQueueStats, MsgQueueError> {
        let queue = self.queues.get(&id).ok_or(MsgQueueError::NotFound)?;
        Ok(queue.stats())
    }

    /// Remove a queue
    pub fn msgctl_rmid(&mut self, id: MsgQueueId) -> Result<(), MsgQueueError> {
        self.queues.remove(&id).ok_or(MsgQueueError::NotFound)?;
        // Remove from key map too
        self.key_map.retain(|_, v| *v != id);
        Ok(())
    }

    /// List all queue IDs
    pub fn list(&self) -> Vec<MsgQueueId> {
        self.queues.keys().copied().collect()
    }
}

impl Default for MsgQueueManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_queue_basic() {
        let mut queue = MessageQueue::new(MsgQueueId(1), 1000, 1000);

        let msg = Message::new(1, b"Hello".to_vec());
        queue.send(msg, 1.0).unwrap();

        let received = queue.receive(0, 2.0).unwrap();
        assert_eq!(received.mtype, 1);
        assert_eq!(received.data, b"Hello");
    }

    #[test]
    fn test_message_type_filtering() {
        let mut queue = MessageQueue::new(MsgQueueId(1), 1000, 1000);

        queue.send(Message::new(1, b"type1".to_vec()), 1.0).unwrap();
        queue.send(Message::new(2, b"type2".to_vec()), 1.0).unwrap();
        queue.send(Message::new(3, b"type3".to_vec()), 1.0).unwrap();

        // Receive type 2 specifically
        let msg = queue.receive(2, 2.0).unwrap();
        assert_eq!(msg.mtype, 2);

        // Receive first available (type 1)
        let msg = queue.receive(0, 3.0).unwrap();
        assert_eq!(msg.mtype, 1);

        // Receive remaining (type 3)
        let msg = queue.receive(0, 4.0).unwrap();
        assert_eq!(msg.mtype, 3);
    }

    #[test]
    fn test_queue_full() {
        let mut queue = MessageQueue::new(MsgQueueId(1), 1000, 1000);
        queue.set_max_bytes(10);

        queue.send(Message::new(1, vec![0; 5]), 1.0).unwrap();
        queue.send(Message::new(1, vec![0; 5]), 1.0).unwrap();

        // Queue is now full
        let result = queue.send(Message::new(1, vec![0; 1]), 1.0);
        assert_eq!(result, Err(MsgQueueError::QueueFull));
    }

    #[test]
    fn test_manager() {
        let mut mgr = MsgQueueManager::new();

        let id1 = mgr.msgget(100, 1000, 1000, true).unwrap();
        let id2 = mgr.msgget(100, 1000, 1000, true).unwrap();
        assert_eq!(id1, id2); // Same key, same ID

        mgr.msgsnd(id1, Message::new(1, b"test".to_vec()), 1.0).unwrap();
        let msg = mgr.msgrcv(id1, 0, 2.0).unwrap();
        assert_eq!(msg.data, b"test");
    }

    #[test]
    fn test_private_queues() {
        let mut mgr = MsgQueueManager::new();

        let id1 = mgr.msgget(-1, 1000, 1000, true).unwrap();
        let id2 = mgr.msgget(-1, 1000, 1000, true).unwrap();
        assert_ne!(id1, id2); // Private queues get unique IDs
    }
}
