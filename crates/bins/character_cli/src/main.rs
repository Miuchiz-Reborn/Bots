use character::{CharacterClient, Notification};
use clap::Parser;
use log::info;
use rustyline::Editor;
use rustyline::error::ReadlineError;

// =================================================================================================
//                                     COMMAND LINE ARGUMENTS
// =================================================================================================

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The host of the character server.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// The port of the character server.
    #[arg(long, default_value = "6675")]
    port: u16,
}

// =================================================================================================
//                                          ENTRYPOINT
// =================================================================================================

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let args = Args::parse();
    let addr = format!("{}:{}", args.host, args.port);

    println!("Connecting to character server at {}", addr);
    let client = CharacterClient::connect(addr)?;
    println!("Successfully connected. Type 'help' for commands.");

    let mut rl = Editor::<(), _>::new()?;
    loop {
        // First, check for any non-blocking notifications from the server.
        match client.check_events() {
            Ok(events) => {
                for event in events {
                    handle_notification(event);
                }
            }
            Err(e) => println!("[Error checking events: {}]", e),
        }

        // Now, get user input.
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str());
                let parts: Vec<&str> = line.trim().split_whitespace().collect();
                if let Some(command) = parts.first() {
                    if !handle_command(command, &parts[1..], &client) {
                        break; // Exit command was received
                    }
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!("Exiting.");
                break;
            }
            Err(err) => {
                println!("[CLI Error: {:?}]", err);
                break;
            }
        }
    }

    Ok(())
}

fn handle_command(command: &str, args: &[&str], client: &CharacterClient) -> bool {
    match command.to_lowercase().as_str() {
        "get_creditz" => {
            let user_id = args.get(0).and_then(|s| s.parse::<u32>().ok());
            if let Some(id) = user_id {
                match client.get_creditz(id) {
                    Ok(value) => println!("Creditz for user {}: {}", id, value),
                    Err(e) => eprintln!("Error getting creditz: {}", e),
                }
            } else {
                eprintln!("Usage: get_creditz <user_id>");
            }
        }
        "set_creditz" => {
            let user_id = args.get(0).and_then(|s| s.parse::<u32>().ok());
            let value = args.get(1).and_then(|s| s.parse::<u32>().ok());
            if let (Some(id), Some(val)) = (user_id, value) {
                match client.set_creditz(id, val) {
                    Ok(_) => println!("Set creditz for user {} to {}", id, val),
                    Err(e) => eprintln!("Error setting creditz: {}", e),
                }
            } else {
                eprintln!("Usage: set_creditz <user_id> <value>");
            }
        }
        "add_creditz" => {
            let user_id = args.get(0).and_then(|s| s.parse::<u32>().ok());
            let amount = args.get(1).and_then(|s| s.parse::<u32>().ok());
            if let (Some(id), Some(amt)) = (user_id, amount) {
                match client.add_creditz(id, amt) {
                    Ok(_) => println!("Added {} creditz to user {}", amt, id),
                    Err(e) => eprintln!("Error adding creditz: {}", e),
                }
            } else {
                eprintln!("Usage: add_creditz <user_id> <amount>");
            }
        }
        "sub_creditz" => {
            let user_id = args.get(0).and_then(|s| s.parse::<u32>().ok());
            let amount = args.get(1).and_then(|s| s.parse::<u32>().ok());
            if let (Some(id), Some(amt)) = (user_id, amount) {
                match client.sub_creditz(id, amt) {
                    Ok(_) => println!("Subtracted {} creditz from user {}", amt, id),
                    Err(e) => eprintln!("Error subtracting creditz: {}", e),
                }
            } else {
                eprintln!("Usage: sub_creditz <user_id> <amount>");
            }
        }
        "get_happiness" => {
            let user_id = args.get(0).and_then(|s| s.parse::<u32>().ok());
            if let Some(id) = user_id {
                match client.get_happiness(id) {
                    Ok(value) => println!("Happiness for user {}: {:.2}", id, value),
                    Err(e) => eprintln!("Error getting happiness: {}", e),
                }
            } else {
                eprintln!("Usage: get_happiness <user_id>");
            }
        }
        "set_happiness" => {
            let user_id = args.get(0).and_then(|s| s.parse::<u32>().ok());
            let value = args.get(1).and_then(|s| s.parse::<f32>().ok());
            if let (Some(id), Some(val)) = (user_id, value) {
                match client.set_happiness(id, val) {
                    Ok(_) => println!("Set happiness for user {} to {:.2}", id, val),
                    Err(e) => eprintln!("Error setting happiness: {}", e),
                }
            } else {
                eprintln!("Usage: set_happiness <user_id> <value>");
            }
        }
        "get_hunger" => {
            let user_id = args.get(0).and_then(|s| s.parse::<u32>().ok());
            if let Some(id) = user_id {
                match client.get_hunger(id) {
                    Ok(value) => println!("Hunger for user {}: {:.2}", id, value),
                    Err(e) => eprintln!("Error getting hunger: {}", e),
                }
            } else {
                eprintln!("Usage: get_hunger <user_id>");
            }
        }
        "set_hunger" => {
            let user_id = args.get(0).and_then(|s| s.parse::<u32>().ok());
            let value = args.get(1).and_then(|s| s.parse::<f32>().ok());
            if let (Some(id), Some(val)) = (user_id, value) {
                match client.set_hunger(id, val) {
                    Ok(_) => println!("Set hunger for user {} to {:.2}", id, val),
                    Err(e) => eprintln!("Error setting hunger: {}", e),
                }
            } else {
                eprintln!("Usage: set_hunger <user_id> <value>");
            }
        }
        "get_boredom" => {
            let user_id = args.get(0).and_then(|s| s.parse::<u32>().ok());
            if let Some(id) = user_id {
                match client.get_boredom(id) {
                    Ok(value) => println!("Boredom for user {}: {:.2}", id, value),
                    Err(e) => eprintln!("Error getting boredom: {}", e),
                }
            } else {
                eprintln!("Usage: get_boredom <user_id>");
            }
        }
        "set_boredom" => {
            let user_id = args.get(0).and_then(|s| s.parse::<u32>().ok());
            let value = args.get(1).and_then(|s| s.parse::<f32>().ok());
            if let (Some(id), Some(val)) = (user_id, value) {
                match client.set_boredom(id, val) {
                    Ok(_) => println!("Set boredom for user {} to {:.2}", id, val),
                    Err(e) => eprintln!("Error setting boredom: {}", e),
                }
            } else {
                eprintln!("Usage: set_boredom <user_id> <value>");
            }
        }
        "help" => {
            println!("Available commands:");
            println!("  get_creditz <user_id>");
            println!("  set_creditz <user_id> <value>");
            println!("  add_creditz <user_id> <amount>");
            println!("  sub_creditz <user_id> <amount>");
            println!("  get_happiness <user_id>");
            println!("  set_happiness <user_id> <value>");
            println!("  get_hunger <user_id>");
            println!("  set_hunger <user_id> <value>");
            println!("  get_boredom <user_id>");
            println!("  set_boredom <user_id> <value>");
            println!("  help");
            println!("  quit");
        }
        "quit" => return false,
        "" => {} // Ignore empty input
        _ => println!("[Unknown command. Type 'help' for a list of commands.]"),
    }
    true
}

fn handle_notification(notification: Notification) {
    match notification {
        Notification::CreditzChanged { user_id, new_value } => {
            println!(
                "\n[Notification] User {}'s creditz changed to {}",
                user_id, new_value
            );
        }
        Notification::HappinessChanged { user_id, new_value } => {
            println!(
                "\n[Notification] User {}'s happiness changed to {:.2}",
                user_id,
                new_value.to_f32()
            );
        }
        Notification::BoredomChanged { user_id, new_value } => {
            println!(
                "\n[Notification] User {}'s boredom changed to {:.2}",
                user_id,
                new_value.to_f32()
            );
        }
        Notification::HungerChanged { user_id, new_value } => {
            println!(
                "\n[Notification] User {}'s hunger changed to {:.2}",
                user_id,
                new_value.to_f32()
            );
        }
    }
}
