use std::{
    collections::HashMap,
    ops::Sub,
    time::{Duration, Instant},
};

use aw_sdk::{
    AvatarChangeInfo, AwEvent, AwInstance, LoginParams, ObjectClickInfo, SdkError, SdkResult,
    StateChangeParams, TeleportParams,
};
use character::CharacterClient;

// =================================================================================================
//                                         CONFIGURATION
// =================================================================================================

// --- Game Settings ---
const TICKET_PRICE: u32 = 5;
const MIN_PLAYERS: usize = 2;
const COUNTDOWN_SECONDS: u64 = 10;
const GAME_DURATION_SECONDS: u64 = 60;
const POST_GAME_SECONDS: u64 = 10;
const GRAND_PRIZE_POINTS: u32 = 50;

// --- World and Position Settings ---
const MION_WORLD: &str = "MION";
const COREMAZE_WORLD: &str = "coremaze";
const COREMAZE_SPAWN_POINT_X: i32 = -5000;
const COREMAZE_SPAWN_POINT_Y: i32 = 0;
const COREMAZE_SPAWN_POINT_Z: i32 = -5500;

const MION_RETURN_SPAWN_POINT_X: i32 = -4660;
const MION_RETURN_SPAWN_POINT_Y: i32 = -5000;
const MION_RETURN_SPAWN_POINT_Z: i32 = 4430;

// Defines the grand prize area as a box from min to max coordinates.
const GRAND_PRIZE_AREA_MIN_X: i32 = 3500;
const GRAND_PRIZE_AREA_MIN_Y: i32 = -10000;
const GRAND_PRIZE_AREA_MIN_Z: i32 = 2850;

const GRAND_PRIZE_AREA_MAX_X: i32 = 6000;
const GRAND_PRIZE_AREA_MAX_Y: i32 = 10000;
const GRAND_PRIZE_AREA_MAX_Z: i32 = 4000;

// --- Advertising Settings ---
const ADVERTISE_NO_PLAYERS_INTERVAL: Duration = Duration::from_secs(10 * 60); // 10 minutes
const ADVERTISE_WAITING_INTERVAL: Duration = Duration::from_secs(60); // 1 minute
const POST_GAME_ADVERTISING_DELAY: Duration = Duration::from_secs(5);

// =================================================================================================
//                                          STATE
// =================================================================================================

#[derive(Debug, Clone)]
struct PlayerInfo {
    citizen_id: u32,
    session_id: u32, // Session ID in the world where they bought the ticket (MION)
    name: String,
}

#[derive(Debug, Clone)]
struct PlayerInGameInfo {
    citizen_id: u32,
    name: String,
    session_id: u32, // Session ID in the maze world
    score: u32,
    has_won_grand_prize: bool,
}

#[derive(Clone)]
enum GamePhase {
    WaitingForPlayers {
        ticket_holders: HashMap<u32, PlayerInfo>, // citizen_id -> PlayerInfo
    },
    Countdown {
        start_time: Instant,
        players: HashMap<u32, PlayerInfo>,
    },
    GameStarting {
        start_time: Instant,
        players: HashMap<u32, PlayerInGameInfo>,
    },
    InProgress {
        start_time: Instant,
        players: HashMap<u32, PlayerInGameInfo>, // citizen_id -> PlayerInGameInfo
    },
    Ending {
        end_time: Instant,
        players: HashMap<u32, PlayerInGameInfo>,
    },
    PostGameCooldown {
        start_time: Instant,
    },
}

struct CoreMazeBot {
    ticket_taker: AwInstance,
    core_maze: AwInstance,
    client: CharacterClient,
    game_phase: GamePhase,
    mion_session_to_citizen: HashMap<u32, u32>,
    coremaze_session_to_citizen: HashMap<u32, u32>,
    last_advertisement: Instant,
}

// =================================================================================================
//                                        IMPLEMENTATION
// =================================================================================================

impl CoreMazeBot {
    fn new() -> Self {
        let ticket_taker =
            AwInstance::new("127.0.0.1", 6670).expect("Failed to create TicketTaker instance");
        let core_maze =
            AwInstance::new("127.0.0.1", 6670).expect("Failed to create CoreMaze instance");
        let client = CharacterClient::connect("127.0.0.1:6675")
            .expect("Failed to connect to character server");

        Self {
            ticket_taker,
            core_maze,
            client,
            game_phase: GamePhase::WaitingForPlayers {
                ticket_holders: HashMap::new(),
            },
            mion_session_to_citizen: HashMap::new(),
            coremaze_session_to_citizen: HashMap::new(),
            last_advertisement: Instant::now().sub(ADVERTISE_NO_PLAYERS_INTERVAL),
        }
    }

    fn run(&mut self) -> SdkResult<()> {
        self.ticket_taker.login(LoginParams::Bot {
            name: "TicketTaker".to_string(),
            owner_id: 1,
            privilege_password: "pass".to_string(),
            application: "CoreMazeBot".to_string(),
        })?;
        self.ticket_taker.enter(MION_WORLD, false)?;
        self.ticket_taker.state_change(StateChangeParams {
            north: 5000,
            height: -4550,
            west: -5000,
            rotation: 0,
            gesture: 0,
            av_type: 20, // InvisibleMan
            av_state: 0,
        })?;
        println!("TicketTaker bot is online in {}", MION_WORLD);

        self.core_maze.login(LoginParams::Bot {
            name: "CoreMazeBot".to_string(),
            owner_id: 1,
            privilege_password: "pass".to_string(),
            application: "CoreMazeBot".to_string(),
        })?;
        self.core_maze.enter(COREMAZE_WORLD, false)?;
        self.core_maze.state_change(StateChangeParams {
            north: 0,
            height: 0,
            west: 0,
            rotation: 0,
            gesture: 0,
            av_type: 0,
            av_state: 0,
        })?;
        println!("CoreMazeBot is online in {}", COREMAZE_WORLD);

        loop {
            self.update_game_state()?;

            let tt_events = self.ticket_taker.tick();
            for event in tt_events {
                self.handle_ticket_taker_event(&event)?;
            }

            let cm_events = self.core_maze.tick();
            for event in cm_events {
                self.handle_core_maze_event(&event)?;
            }

            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn update_game_state(&mut self) -> SdkResult<()> {
        let current_phase = self.game_phase.clone();
        match current_phase {
            GamePhase::WaitingForPlayers { ticket_holders } => {
                if ticket_holders.len() >= MIN_PLAYERS {
                    self.ticket_taker.say(&format!(
                        "Starting game for CoreMaze with {} players! Teleporting in {} seconds...",
                        ticket_holders.len(),
                        COUNTDOWN_SECONDS
                    ))?;
                    self.game_phase = GamePhase::Countdown {
                        start_time: Instant::now(),
                        players: ticket_holders,
                    };
                    return Ok(()); // Return early to avoid advertising right after starting
                }

                // Advertising logic
                let elapsed = self.last_advertisement.elapsed();
                if ticket_holders.is_empty() {
                    if elapsed >= ADVERTISE_NO_PLAYERS_INTERVAL {
                        self.ticket_taker.say(
                            "Tickets are now available for CoreMaze - Solve the maze and win!",
                        )?;
                        self.last_advertisement = Instant::now();
                    }
                } else if elapsed >= ADVERTISE_WAITING_INTERVAL {
                    let needed = MIN_PLAYERS - ticket_holders.len();
                    self.ticket_taker.say(&format!(
                        "CoreMaze needs more players - come sign up! I have {}, and need {} more",
                        ticket_holders.len(),
                        needed
                    ))?;
                    self.last_advertisement = Instant::now();
                }
            }
            GamePhase::Countdown {
                start_time,
                players,
            } => {
                if start_time.elapsed() >= Duration::from_secs(COUNTDOWN_SECONDS) {
                    // Teleport all players to the maze
                    for player in players.values() {
                        self.ticket_taker.teleport(TeleportParams {
                            session_id: player.session_id,
                            world: COREMAZE_WORLD.to_string(),
                            north: COREMAZE_SPAWN_POINT_Z,
                            height: COREMAZE_SPAWN_POINT_Y,
                            west: COREMAZE_SPAWN_POINT_X,
                            rotation: 0,
                            warp: false,
                        })?;
                    }

                    // Welcome message in the maze is now delayed.

                    // Prepare the in-game players map
                    let in_game_players = players
                        .into_iter()
                        .map(|(id, info)| {
                            (
                                id,
                                PlayerInGameInfo {
                                    citizen_id: id,
                                    name: info.name, // Pre-fill name
                                    session_id: 0,   // Will be filled in on AvatarAdd
                                    score: 0,
                                    has_won_grand_prize: false,
                                },
                            )
                        })
                        .collect();

                    // Change game phase to GameStarting
                    self.game_phase = GamePhase::GameStarting {
                        start_time: Instant::now(),
                        players: in_game_players,
                    };
                }
            }
            GamePhase::GameStarting {
                start_time,
                players,
            } => {
                if start_time.elapsed() >= Duration::from_secs(2) {
                    // Welcome message in the maze
                    self.core_maze.say(
                        "Welcome to Maze!  Try to collect as many points you can by running in to prize objects.  First one to the end of the maze wins!",
                    )?;
                    self.game_phase = GamePhase::InProgress {
                        start_time: Instant::now(), // Reset start time for game duration
                        players,
                    }
                }
            }
            GamePhase::InProgress { start_time, .. } => {
                if start_time.elapsed() >= Duration::from_secs(GAME_DURATION_SECONDS) {
                    self.core_maze.say("Game has ended! Tallying scores...")?;
                    if let GamePhase::InProgress { players, .. } = self.game_phase.clone() {
                        self.end_game(players)?;
                    }
                }
            }
            GamePhase::Ending { end_time, .. } => {
                if end_time.elapsed() >= Duration::from_secs(POST_GAME_SECONDS) {
                    // Teleport everyone back to MION
                    if let GamePhase::Ending { players, .. } = &self.game_phase {
                        for player in players.values() {
                            self.core_maze.teleport(TeleportParams {
                                session_id: player.session_id,
                                world: MION_WORLD.to_string(),
                                north: MION_RETURN_SPAWN_POINT_Z,
                                height: MION_RETURN_SPAWN_POINT_Y,
                                west: MION_RETURN_SPAWN_POINT_X,
                                rotation: 0,
                                warp: false,
                            })?;
                        }
                    }

                    // Reset for the next game
                    self.ticket_taker
                        .say("A new game of CoreMaze will begin shortly. Tickets are available!")?;

                    self.game_phase = GamePhase::PostGameCooldown {
                        start_time: Instant::now(),
                    };
                }
            }
            GamePhase::PostGameCooldown { start_time } => {
                if start_time.elapsed() >= POST_GAME_ADVERTISING_DELAY {
                    self.game_phase = GamePhase::WaitingForPlayers {
                        ticket_holders: HashMap::new(),
                    };
                    // Set the last advertisement time to the distant past so that the first
                    // announcement happens immediately after the cooldown.
                    self.last_advertisement = Instant::now().sub(ADVERTISE_NO_PLAYERS_INTERVAL);
                }
            }
        }
        Ok(())
    }

    fn handle_ticket_taker_event(&mut self, event: &AwEvent) -> SdkResult<()> {
        match event {
            AwEvent::AvatarAdd(avatar_add) => {
                if let Some(citizen_id) = avatar_add.citizen_id {
                    self.mion_session_to_citizen
                        .insert(avatar_add.session_id, citizen_id);
                }
            }
            AwEvent::AvatarDelete(avatar_delete) => {
                self.mion_session_to_citizen
                    .remove(&avatar_delete.session_id);
            }
            AwEvent::ObjectClick(click) => {
                if click.object_info.action.contains("~TicketTaker=CoreMaze~") {
                    self.handle_ticket_purchase(click)?;
                }
            }
            AwEvent::UniverseDisconnected | AwEvent::WorldDisconnected => {
                return Err(SdkError::connection_state("Universe or world disconnected"));
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_core_maze_event(&mut self, event: &AwEvent) -> SdkResult<()> {
        match event {
            AwEvent::AvatarAdd(avatar_add) => {
                if let GamePhase::InProgress { players, .. }
                | GamePhase::GameStarting { players, .. } = &mut self.game_phase
                {
                    let Some(citizen_id) = avatar_add.citizen_id else {
                        return Ok(());
                    };

                    self.coremaze_session_to_citizen
                        .insert(avatar_add.session_id, citizen_id);

                    // If this player is expected in the game, update their info
                    if let Some(player_info) = players.get_mut(&citizen_id) {
                        player_info.name = avatar_add.name.clone();
                        player_info.session_id = avatar_add.session_id;
                    }
                }
            }
            AwEvent::AvatarDelete(avatar_delete) => {
                self.coremaze_session_to_citizen
                    .remove(&avatar_delete.session_id);
            }
            AwEvent::AvatarChange(avatar_change) => {
                if let GamePhase::InProgress { players, .. } = &mut self.game_phase {
                    let pos_x = avatar_change.west;
                    let pos_y = avatar_change.height;
                    let pos_z = avatar_change.north;

                    if pos_x >= GRAND_PRIZE_AREA_MIN_X
                        && pos_x <= GRAND_PRIZE_AREA_MAX_X
                        && pos_y >= GRAND_PRIZE_AREA_MIN_Y
                        && pos_y <= GRAND_PRIZE_AREA_MAX_Y
                        && pos_z >= GRAND_PRIZE_AREA_MIN_Z
                        && pos_z <= GRAND_PRIZE_AREA_MAX_Z
                    {
                        if let Some(citizen_id) = self
                            .coremaze_session_to_citizen
                            .get(&avatar_change.session_id)
                        {
                            if let Some(player) = players.get_mut(citizen_id) {
                                if !player.has_won_grand_prize {
                                    player.score += GRAND_PRIZE_POINTS;
                                    player.has_won_grand_prize = true;
                                    self.core_maze.say(&format!(
                                        "{} has found the grand prize and gets {} points!",
                                        player.name, GRAND_PRIZE_POINTS
                                    ))?;
                                }
                            }
                        }
                    }
                }
            }
            AwEvent::UniverseDisconnected | AwEvent::WorldDisconnected => {
                return Err(SdkError::connection_state("Universe or world disconnected"));
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_ticket_purchase(&mut self, click: &ObjectClickInfo) -> SdkResult<()> {
        if let GamePhase::WaitingForPlayers { ticket_holders } = &mut self.game_phase {
            let Some(citizen_id) = self
                .mion_session_to_citizen
                .get(&click.avatar_session)
                .cloned()
            else {
                // We don't have a citizen ID for this session, so ignore.
                return Ok(());
            };

            if ticket_holders.contains_key(&citizen_id) {
                // Player is returning a ticket
                self.client.add_creditz(citizen_id, TICKET_PRICE).ok();
                ticket_holders.remove(&citizen_id);
                self.ticket_taker
                    .say(&format!("{} has returned their ticket.", click.avatar_name))?;
            } else {
                // Player is buying a ticket
                match self.client.sub_creditz(citizen_id, TICKET_PRICE) {
                    Ok(_) => {
                        let player_info = PlayerInfo {
                            citizen_id,
                            session_id: click.avatar_session,
                            name: click.avatar_name.clone(),
                        };
                        ticket_holders.insert(citizen_id, player_info);
                        self.ticket_taker.say(&format!(
                            "{} has bought a ticket for CoreMaze! We now have {} players.",
                            click.avatar_name,
                            ticket_holders.len()
                        ))?;
                    }
                    Err(_) => {
                        self.ticket_taker.say(&format!(
                            "Sorry {}, you don't have enough creditz to buy a ticket.",
                            click.avatar_name
                        ))?;
                    }
                }
            }
        } else {
            self.ticket_taker.say(&format!(
                "Sorry {}, tickets are not available for purchase right now.",
                click.avatar_name
            ))?;
        }
        Ok(())
    }

    fn end_game(&mut self, players: HashMap<u32, PlayerInGameInfo>) -> SdkResult<()> {
        self.core_maze.say("Here are the final scores:")?;
        for player in players.values() {
            self.core_maze.say(&format!(
                "{} collected {} points",
                player.name, player.score
            ))?;
            self.client
                .add_creditz(player.citizen_id, player.score)
                .ok();
            if let Ok(mut happiness) = self.client.get_happiness(player.citizen_id) {
                happiness += 0.1;
                self.client.set_happiness(player.citizen_id, happiness).ok();
            }
            if let Ok(mut boredom) = self.client.get_boredom(player.citizen_id) {
                boredom += 0.25;
                self.client.set_boredom(player.citizen_id, boredom).ok();
            }
        }

        self.core_maze
            .say("Thanks for playing!  I'll send you home in a few seconds :)")?;

        self.game_phase = GamePhase::Ending {
            end_time: Instant::now(),
            players,
        };

        Ok(())
    }
}

// =================================================================================================
//                                          ENTRYPOINT
// =================================================================================================

fn main() {
    loop {
        let mut bot = CoreMazeBot::new();
        if let Err(e) = bot.run() {
            println!("Bot encountered an error: {:?}. Restarting.", e);
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}
