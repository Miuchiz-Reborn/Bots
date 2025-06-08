use crate::error::CharacterError;
use crate::protocol::{Notification, Request, Response, ServerMessage};
use log::{info, warn};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Mutex;
use std::time::Duration;

/// A client for interacting with the character server.
pub struct CharacterClient {
    server_addr: String,
    stream: Mutex<TcpStream>,
    notification_buffer: Mutex<VecDeque<Notification>>,
}

impl CharacterClient {
    /// Connects to the character server and returns a new client.
    /// This will block until a connection is established.
    pub fn connect<A: ToSocketAddrs>(addr: A) -> Result<Self, CharacterError> {
        let server_addr = addr.to_socket_addrs()?.next().unwrap().to_string();
        let stream = Self::establish_connection(&server_addr)?;
        Ok(Self {
            server_addr,
            stream: Mutex::new(stream),
            notification_buffer: Mutex::new(VecDeque::new()),
        })
    }

    /// The internal reconnect loop.
    fn establish_connection(addr: &str) -> Result<TcpStream, CharacterError> {
        loop {
            info!("Attempting to connect to server at {}...", addr);
            match TcpStream::connect(addr) {
                Ok(stream) => {
                    info!("Successfully connected to server.");
                    // Do NOT set a global read timeout. Let `request` calls block.
                    return Ok(stream);
                }
                Err(e) => {
                    warn!("Connection failed: {}. Retrying in 5 seconds...", e);
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        }
    }

    /// A helper to send a request and receive a response, with reconnect logic.
    fn request(&self, request: Request) -> Result<Response, CharacterError> {
        let mut stream_lock = self.stream.lock().unwrap();
        let payload = bincode::serialize(&request)?;

        'retry_loop: loop {
            // Attempt to write the payload.
            if let Err(e) = write_frame(&mut *stream_lock, &payload) {
                warn!("Failed to send request: {}. Reconnecting...", e);
                *stream_lock = Self::establish_connection(&self.server_addr)?;
                continue 'retry_loop; // Retry write
            }

            // Attempt to read a response, handling notifications that may arrive first.
            loop {
                match read_frame(&mut *stream_lock) {
                    Ok(response_payload) => {
                        match bincode::deserialize::<ServerMessage>(&response_payload)? {
                            ServerMessage::Response(response) => {
                                // This is the direct response we were waiting for.
                                return Ok(response);
                            }
                            ServerMessage::Notification(notification) => {
                                // This is a broadcasted notification. Buffer it and keep waiting.
                                self.notification_buffer
                                    .lock()
                                    .unwrap()
                                    .push_back(notification);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read response: {}. Reconnecting...", e);
                        *stream_lock = Self::establish_connection(&self.server_addr)?;
                        // After reconnecting, the original request must be resent.
                        continue 'retry_loop;
                    }
                }
            }
        }
    }

    /// Checks for any pending notifications from the server.
    /// This is a non-blocking check.
    pub fn check_events(&self) -> Result<Vec<Notification>, CharacterError> {
        // First, drain any notifications that were buffered during a previous request.
        let mut notifications: Vec<_> =
            self.notification_buffer.lock().unwrap().drain(..).collect();

        let mut stream_lock = self.stream.lock().unwrap();

        // Temporarily set a non-blocking read timeout for this check.
        stream_lock.set_read_timeout(Some(Duration::from_millis(10)))?;

        loop {
            match read_frame(&mut *stream_lock) {
                Ok(payload) => {
                    // We assume any message read here is a notification.
                    // If it's a Response, it's a protocol error, as we aren't in a request.
                    match bincode::deserialize::<ServerMessage>(&payload)? {
                        ServerMessage::Notification(notification) => {
                            notifications.push(notification);
                        }
                        ServerMessage::Response(_) => {
                            warn!("Received unexpected Response outside of a request cycle.");
                            // We still need to restore the timeout before returning.
                            let _ = stream_lock.set_read_timeout(None);
                            return Err(CharacterError::UnexpectedPacket);
                        }
                    }
                }
                Err(CharacterError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // This is expected when there are no more events.
                    break;
                }
                Err(e) => {
                    // A real error occurred. Restore blocking and then reconnect.
                    let _ = stream_lock.set_read_timeout(None);
                    warn!("Error checking events: {}. Reconnecting...", e);
                    *stream_lock = Self::establish_connection(&self.server_addr)?;
                    return Ok(Vec::new());
                }
            }
        }

        // IMPORTANT: Restore the default blocking behavior for subsequent requests.
        stream_lock.set_read_timeout(None)?;

        Ok(notifications)
    }

    // --- Public API Methods ---

    pub fn get_creditz(&self, user_id: u32) -> Result<u32, CharacterError> {
        let request = Request::GetCreditz(user_id);
        match self.request(request)? {
            Response::Creditz(value) => Ok(value),
            Response::Error(e) => Err(CharacterError::Server(e)),
            _ => Err(CharacterError::UnexpectedPacket),
        }
    }

    pub fn set_creditz(&self, user_id: u32, value: u32) -> Result<(), CharacterError> {
        let request = Request::SetCreditz(user_id, value);
        match self.request(request)? {
            Response::Success => Ok(()),
            Response::Error(e) => Err(CharacterError::Server(e)),
            _ => Err(CharacterError::UnexpectedPacket),
        }
    }

    pub fn add_creditz(&self, user_id: u32, amount: u32) -> Result<(), CharacterError> {
        let request = Request::AddCreditz(user_id, amount);
        match self.request(request)? {
            Response::Success => Ok(()),
            Response::Error(e) => Err(CharacterError::Server(e)),
            _ => Err(CharacterError::UnexpectedPacket),
        }
    }

    pub fn sub_creditz(&self, user_id: u32, amount: u32) -> Result<(), CharacterError> {
        let request = Request::SubtractCreditz(user_id, amount);
        match self.request(request)? {
            Response::Success => Ok(()),
            Response::Error(e) => Err(CharacterError::Server(e)),
            _ => Err(CharacterError::UnexpectedPacket),
        }
    }

    // --- Stats ---

    pub fn get_happiness(&self, user_id: u32) -> Result<f32, CharacterError> {
        let request = Request::GetHappiness(user_id);
        match self.request(request)? {
            Response::Happiness(value) => Ok(value.to_f32()),
            Response::Error(e) => Err(CharacterError::Server(e)),
            _ => Err(CharacterError::UnexpectedPacket),
        }
    }

    pub fn set_happiness(&self, user_id: u32, value: f32) -> Result<(), CharacterError> {
        let request = Request::SetHappiness(user_id, value);
        match self.request(request)? {
            Response::Success => Ok(()),
            Response::Error(e) => Err(CharacterError::Server(e)),
            _ => Err(CharacterError::UnexpectedPacket),
        }
    }

    pub fn get_hunger(&self, user_id: u32) -> Result<f32, CharacterError> {
        let request = Request::GetHunger(user_id);
        match self.request(request)? {
            Response::Hunger(value) => Ok(value.to_f32()),
            Response::Error(e) => Err(CharacterError::Server(e)),
            _ => Err(CharacterError::UnexpectedPacket),
        }
    }

    pub fn set_hunger(&self, user_id: u32, value: f32) -> Result<(), CharacterError> {
        let request = Request::SetHunger(user_id, value);
        match self.request(request)? {
            Response::Success => Ok(()),
            Response::Error(e) => Err(CharacterError::Server(e)),
            _ => Err(CharacterError::UnexpectedPacket),
        }
    }

    pub fn get_boredom(&self, user_id: u32) -> Result<f32, CharacterError> {
        let request = Request::GetBoredom(user_id);
        match self.request(request)? {
            Response::Boredom(value) => Ok(value.to_f32()),
            Response::Error(e) => Err(CharacterError::Server(e)),
            _ => Err(CharacterError::UnexpectedPacket),
        }
    }

    pub fn set_boredom(&self, user_id: u32, value: f32) -> Result<(), CharacterError> {
        let request = Request::SetBoredom(user_id, value);
        match self.request(request)? {
            Response::Success => Ok(()),
            Response::Error(e) => Err(CharacterError::Server(e)),
            _ => Err(CharacterError::UnexpectedPacket),
        }
    }
}

// --- Framing Helpers ---

/// Writes a bincode-serialized payload to the stream with a 4-byte length prefix.
fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<(), CharacterError> {
    let len = payload.len() as u32;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(payload)?;
    Ok(())
}

/// Reads a frame from the stream, expecting a 4-byte length prefix.
fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, CharacterError> {
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes)?;
    let len = u32::from_be_bytes(len_bytes);

    let mut buffer = vec![0u8; len as usize];
    stream.read_exact(&mut buffer)?;
    Ok(buffer)
}
