use thiserror::Error;

#[derive(Error, Debug)]
pub enum CharacterError {
    #[error("Network I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization/deserialization error: {0}")]
    Bincode(#[from] Box<bincode::ErrorKind>),
    #[error("Server returned an error: {0}")]
    Server(String),
    #[error("Received an unexpected packet from the server")]
    UnexpectedPacket,
    #[error("Connection was closed")]
    ConnectionClosed,
}
