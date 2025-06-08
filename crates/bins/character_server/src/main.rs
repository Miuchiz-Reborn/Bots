use bytes::Bytes;
use character::{Notification, Request, Response, ServerMessage, StatBar};
use clap::Parser;
use log::{error, info, warn};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

mod config;
use config::CharacterServerConfig;
mod database;
use database::MiuchizDatabase;

// =================================================================================================
//                                     COMMAND LINE ARGUMENTS
// =================================================================================================

#[derive(Parser, Debug)]
struct Args {
    /// Path to the TOML configuration file.
    #[arg(short, long)]
    config: PathBuf,
}

// =================================================================================================
//                                         SERVER STATE
// =================================================================================================

/// A map of connected client addresses to a sender for their dedicated message channel.
type ClientMap = Arc<Mutex<HashMap<SocketAddr, mpsc::Sender<Bytes>>>>;
/// The shared, thread-safe database connection, protected by a Tokio Mutex.
type Db = Arc<Mutex<MiuchizDatabase>>;

// =================================================================================================
//                                          ENTRYPOINT
// =================================================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let args = Args::parse();
    let config = toml::from_str::<CharacterServerConfig>(&std::fs::read_to_string(args.config)?)?;

    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;
    info!("Character server listening on {}", addr);

    // Initialize the real database connection from the config.
    info!("Connecting to database...");
    let db = MiuchizDatabase::new(config.database);
    let db = Arc::new(Mutex::new(db)); // Wrap in Arc<Mutex> for thread safety

    info!("Database connection successful.");

    // Initialize shared state for clients
    let clients = ClientMap::new(Mutex::new(HashMap::new()));

    loop {
        let (stream, addr) = listener.accept().await?;
        let clients_clone = clients.clone();
        let db_clone = db.clone();

        tokio::spawn(async move {
            info!("Accepted connection from: {}", addr);
            if let Err(e) = handle_connection(stream, addr, clients_clone, db_clone).await {
                error!("Error handling connection from {}: {}", addr, e);
            }
        });
    }
}

// =================================================================================================
//                                       CONNECTION HANDLING
// =================================================================================================

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    clients: ClientMap,
    db: Db,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (tx, mut rx) = mpsc::channel::<Bytes>(32);

    // Add the new client's message sender to the shared map.
    clients.lock().await.insert(addr, tx.clone());

    loop {
        tokio::select! {
            // Read data from the client's socket
            result = reader.read_u32() => {
                let len = match result {
                    Ok(len) => len,
                    Err(_) => break, // Client disconnected
                };

                let mut buffer = vec![0; len as usize];
                if reader.read_exact(&mut buffer).await.is_err() {
                    break; // Client disconnected
                }

                // Process the request using the real database.
                let request: Request = bincode::deserialize(&buffer)?;
                let db_clone = db.clone();
                let (response, notification) =
                    tokio::task::spawn_blocking(move || process_request(request, &db_clone))
                        .await?;

                // Send the direct response back to the requester via its channel.
                let response_payload = bincode::serialize(&ServerMessage::Response(response))?;
                if tx.send(response_payload.into()).await.is_err() {
                    break; // Channel closed
                }

                // If there was a state change, broadcast the notification to all clients.
                if let Some(notif) = notification {
                    let notif_payload = bincode::serialize(&ServerMessage::Notification(notif))?;
                    broadcast_notification(&clients, &notif_payload.into()).await;
                }
            },
            // Receive messages from other tasks to be written to this client's socket
            Some(payload) = rx.recv() => {
                if write_frame(&mut writer, &payload).await.is_err() {
                    break; // Failed to write to client
                }
            }
        }
    }

    // On disconnect, remove the client from the map.
    info!("Closing connection from: {}", addr);
    clients.lock().await.remove(&addr);
    Ok(())
}

fn process_request(request: Request, db: &Db) -> (Response, Option<Notification>) {
    // Lock the mutex to gain exclusive access to the database.
    let db_lock = db.blocking_lock();

    let user_id = match &request {
        Request::GetCreditz(id)
        | Request::SetCreditz(id, _)
        | Request::AddCreditz(id, _)
        | Request::SubtractCreditz(id, _)
        | Request::GetHappiness(id)
        | Request::SetHappiness(id, _)
        | Request::GetHunger(id)
        | Request::SetHunger(id, _)
        | Request::GetBoredom(id)
        | Request::SetBoredom(id, _) => *id,
    };

    // Every operation should ensure the user exists in the database first.
    if let database::DatabaseResult::DatabaseError = db_lock.init_player_if_not_exists(user_id) {
        return (
            Response::Error("Database error: Could not initialize player.".to_string()),
            None,
        );
    }

    match request {
        Request::GetCreditz(user_id) => match db_lock.get_stats(user_id) {
            database::DatabaseResult::Ok(stats) => (Response::Creditz(stats.creditz), None),
            database::DatabaseResult::DatabaseError => (
                Response::Error("Database error: Failed to retrieve stats.".to_string()),
                None,
            ),
        },
        Request::SetCreditz(user_id, value) => {
            let mut stats = match db_lock.get_stats(user_id) {
                database::DatabaseResult::Ok(s) => s,
                database::DatabaseResult::DatabaseError => {
                    return (Response::Error("DB Error".into()), None)
                }
            };
            stats.creditz = value;
            match db_lock.set_stats(user_id, stats) {
                database::DatabaseResult::Ok(_) => (
                    Response::Success,
                    Some(Notification::CreditzChanged {
                        user_id,
                        new_value: value,
                    }),
                ),
                database::DatabaseResult::DatabaseError => {
                    (Response::Error("DB Error".into()), None)
                }
            }
        }
        Request::AddCreditz(user_id, amount) => {
            let mut stats = match db_lock.get_stats(user_id) {
                database::DatabaseResult::Ok(s) => s,
                database::DatabaseResult::DatabaseError => {
                    return (Response::Error("DB Error".into()), None)
                }
            };
            stats.creditz += amount;
            let new_value = stats.creditz;
            match db_lock.set_stats(user_id, stats) {
                database::DatabaseResult::Ok(_) => (
                    Response::Success,
                    Some(Notification::CreditzChanged { user_id, new_value }),
                ),
                database::DatabaseResult::DatabaseError => {
                    (Response::Error("DB Error".into()), None)
                }
            }
        }
        Request::SubtractCreditz(user_id, amount) => {
            let mut stats = match db_lock.get_stats(user_id) {
                database::DatabaseResult::Ok(s) => s,
                database::DatabaseResult::DatabaseError => {
                    return (Response::Error("DB Error".into()), None)
                }
            };
            if stats.creditz < amount {
                return (Response::Error("Insufficient funds".to_string()), None);
            }
            stats.creditz -= amount;
            let new_value = stats.creditz;
            match db_lock.set_stats(user_id as u32, stats) {
                database::DatabaseResult::Ok(_) => (
                    Response::Success,
                    Some(Notification::CreditzChanged { user_id, new_value }),
                ),
                database::DatabaseResult::DatabaseError => {
                    (Response::Error("DB Error".into()), None)
                }
            }
        }
        Request::GetHappiness(user_id) => match db_lock.get_stats(user_id) {
            database::DatabaseResult::Ok(s) => (Response::Happiness(s.happiness), None),
            database::DatabaseResult::DatabaseError => (Response::Error("DB Error".into()), None),
        },
        Request::SetHappiness(user_id, value) => {
            let mut stats = match db_lock.get_stats(user_id) {
                database::DatabaseResult::Ok(s) => s,
                database::DatabaseResult::DatabaseError => {
                    return (Response::Error("DB Error".into()), None)
                }
            };
            stats.happiness = StatBar::from_f32(value);
            match db_lock.set_stats(user_id, stats) {
                database::DatabaseResult::Ok(_) => (
                    Response::Success,
                    Some(Notification::HappinessChanged {
                        user_id,
                        new_value: StatBar::from_f32(value),
                    }),
                ),
                database::DatabaseResult::DatabaseError => {
                    (Response::Error("DB Error".into()), None)
                }
            }
        }
        Request::GetHunger(user_id) => match db_lock.get_stats(user_id) {
            database::DatabaseResult::Ok(s) => (Response::Hunger(s.hunger), None),
            database::DatabaseResult::DatabaseError => (Response::Error("DB Error".into()), None),
        },
        Request::SetHunger(user_id, value) => {
            let mut stats = match db_lock.get_stats(user_id) {
                database::DatabaseResult::Ok(s) => s,
                database::DatabaseResult::DatabaseError => {
                    return (Response::Error("DB Error".into()), None)
                }
            };
            stats.hunger = StatBar::from_f32(value);
            match db_lock.set_stats(user_id, stats) {
                database::DatabaseResult::Ok(_) => (
                    Response::Success,
                    Some(Notification::HungerChanged {
                        user_id,
                        new_value: StatBar::from_f32(value),
                    }),
                ),
                database::DatabaseResult::DatabaseError => {
                    (Response::Error("DB Error".into()), None)
                }
            }
        }
        Request::GetBoredom(user_id) => match db_lock.get_stats(user_id) {
            database::DatabaseResult::Ok(s) => (Response::Boredom(s.boredom), None),
            database::DatabaseResult::DatabaseError => (Response::Error("DB Error".into()), None),
        },
        Request::SetBoredom(user_id, value) => {
            let mut stats = match db_lock.get_stats(user_id) {
                database::DatabaseResult::Ok(s) => s,
                database::DatabaseResult::DatabaseError => {
                    return (Response::Error("DB Error".into()), None)
                }
            };
            stats.boredom = StatBar::from_f32(value);
            match db_lock.set_stats(user_id, stats) {
                database::DatabaseResult::Ok(_) => (
                    Response::Success,
                    Some(Notification::BoredomChanged {
                        user_id,
                        new_value: StatBar::from_f32(value),
                    }),
                ),
                database::DatabaseResult::DatabaseError => {
                    (Response::Error("DB Error".into()), None)
                }
            }
        }
    }
}

async fn broadcast_notification(clients: &ClientMap, payload: &Bytes) {
    let mut dead_clients = Vec::new();

    // Clone the senders to avoid holding the lock across an .await point.
    let senders: Vec<(SocketAddr, mpsc::Sender<Bytes>)> = clients
        .lock()
        .await
        .iter()
        .map(|(addr, tx)| (*addr, tx.clone()))
        .collect();

    // Now, iterate over the cloned senders without holding the lock.
    for (addr, tx) in senders {
        if tx.send(payload.clone()).await.is_err() {
            // The channel is closed, meaning the client's task has terminated.
            warn!(
                "Failed to send notification to {}: channel closed. Marking for removal.",
                addr
            );
            dead_clients.push(addr);
        }
    }

    // Re-acquire the lock briefly just to clean up any dead clients.
    if !dead_clients.is_empty() {
        let mut clients_lock = clients.lock().await;
        for addr in dead_clients {
            clients_lock.remove(&addr);
        }
    }
}

/// Helper to write a length-prefixed frame asynchronously.
async fn write_frame(
    writer: &mut tokio::io::WriteHalf<TcpStream>,
    payload: &[u8],
) -> Result<(), std::io::Error> {
    writer.write_u32(payload.len() as u32).await?;
    writer.write_all(payload).await?;
    Ok(())
}
