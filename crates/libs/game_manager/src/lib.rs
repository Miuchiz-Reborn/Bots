//! The generic, reusable logic for a two-bot ticket and game start system.

use std::{
    collections::HashMap,
    ops::Sub,
    time::{Duration, Instant},
};

use aw_sdk::{
    AwEvent, AwInstance, ConsoleMessageParams, ObjectClickInfo, ObjectInfo, QueryResult, SdkError,
    SdkResult, StateChangeParams, TeleportParams,
};
use character::CharacterClient;

// =================================================================================================
//                                         CONFIGURATION
// =================================================================================================

pub struct GameConfig {
    pub game_name: String,
    pub tagline: Option<String>,
    pub ticket_price: u32,
    pub min_players: usize,
    pub wait_for_more_players_seconds: u64,
    pub countdown_seconds: u64,
    pub ad_no_players_interval: Duration,
    pub ad_waiting_interval: Duration,
    pub ad_post_game_delay: Duration,

    pub ticket_world_name: String,
    pub game_world_name: String,

    pub ticket_taker_action: String,

    pub ticket_taker_pos: (i32, i32, i32),
    pub game_spawn_pos: (i32, i32, i32, i32),
    pub mion_return_spawn_pos: (i32, i32, i32, i32),
}

// =================================================================================================
//                                             STATE
// =================================================================================================

#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub citizen_id: u32,
    pub session_id: u32,
    pub name: String,
}

enum Phase {
    Waiting,
    WaitingForMore { start_time: Instant },
    Countdown { start_time: Instant },
    PostGameCooldown { start_time: Instant },
}

impl Default for Phase {
    fn default() -> Self {
        Phase::Waiting
    }
}

pub struct GameManager {
    ticket_taker: AwInstance,
    client: CharacterClient,
    config: GameConfig,
    phase: Phase,
    ticket_holders: HashMap<u32, PlayerInfo>,
    mion_session_to_citizen: HashMap<u32, u32>,
    last_advertisement: Instant,
}

// =================================================================================================
//                                        IMPLEMENTATION
// =================================================================================================

impl GameManager {
    pub fn new(
        host: &str,
        port: u16,
        character_addr: &str,
        config: GameConfig,
    ) -> Result<Self, String> {
        let ticket_taker =
            AwInstance::new(host, port).map_err(|e| format!("TicketTaker: {}", e))?;
        let client = CharacterClient::connect(character_addr)
            .map_err(|e| format!("CharacterClient: {}", e))?;

        Ok(Self {
            ticket_taker,
            client,
            config,
            phase: Phase::default(),
            ticket_holders: HashMap::new(),
            mion_session_to_citizen: HashMap::new(),
            last_advertisement: Instant::now().sub(Duration::from_secs(60 * 60)), // In the past
        })
    }

    pub fn login(&mut self, owner_id: u32, priv_pass: &str) -> SdkResult<()> {
        self.ticket_taker.login(aw_sdk::LoginParams::Bot {
            name: "TicketTaker".to_string(),
            owner_id,
            privilege_password: priv_pass.to_string(),
            application: "Bot".to_string(),
        })?;
        self.ticket_taker
            .enter(&self.config.ticket_world_name, false)?;
        let (x, y, z) = self.config.ticket_taker_pos;
        self.ticket_taker.state_change(StateChangeParams {
            west: x,
            height: y,
            north: z,
            rotation: 0,
            gesture: 0,
            av_type: 20,
            av_state: 0,
        })?;

        Ok(())
    }

    pub fn tick(&mut self) -> SdkResult<Option<HashMap<u32, PlayerInfo>>> {
        for event in self.ticket_taker.tick() {
            self.handle_ticket_taker_event(&event)?;
        }
        self.update_phase()
    }

    pub fn game_is_over(&mut self) -> SdkResult<()> {
        self.phase = Phase::PostGameCooldown {
            start_time: Instant::now(),
        };
        self.ticket_holders.clear();
        let suffix = match &self.config.tagline {
            Some(tagline) => format!(" - {}", tagline),
            None => "".to_string(),
        };
        self.ticket_taker.say(&format!(
            "Tickets are now available for {}{}",
            self.config.game_name, suffix
        ))?;
        Ok(())
    }

    pub fn teleport_to_game(&mut self, players: &HashMap<u32, PlayerInfo>) -> SdkResult<()> {
        let (x, y, z, rot) = self.config.game_spawn_pos;
        for player in players.values() {
            self.ticket_taker.teleport(TeleportParams {
                session_id: player.session_id,
                world: self.config.game_world_name.clone(),
                north: z,
                height: y,
                west: x,
                rotation: rot,
                warp: false,
            })?;
        }
        Ok(())
    }

    pub fn query(&mut self, sector_x: i32, sector_z: i32) -> SdkResult<QueryResult> {
        self.ticket_taker.query(sector_x, sector_z)
    }

    pub fn object_change(&mut self, object: ObjectInfo) -> SdkResult<()> {
        self.ticket_taker.object_change(object)
    }

    fn update_phase(&mut self) -> SdkResult<Option<HashMap<u32, PlayerInfo>>> {
        let current_phase = &self.phase;
        match current_phase {
            Phase::Waiting => {
                if self.ticket_holders.len() >= self.config.min_players {
                    self.ticket_taker.say(&format!(
                        "{} will start in {} seconds! Get a ticket now!",
                        self.config.game_name, self.config.wait_for_more_players_seconds
                    ))?;
                    self.phase = Phase::WaitingForMore {
                        start_time: Instant::now(),
                    };
                    return Ok(None);
                }

                let elapsed = self.last_advertisement.elapsed();
                if self.ticket_holders.is_empty() {
                    if elapsed >= self.config.ad_no_players_interval {
                        let suffix = match &self.config.tagline {
                            Some(tagline) => format!(" - {}", tagline),
                            None => "".to_string(),
                        };
                        self.ticket_taker.say(&format!(
                            "Tickets are now available for {}{}",
                            self.config.game_name, suffix
                        ))?;
                        self.last_advertisement = Instant::now();
                    }
                } else if elapsed >= self.config.ad_waiting_interval {
                    let needed = self.config.min_players - self.ticket_holders.len();
                    self.ticket_taker.say(&format!(
                        "{} needs more players - come sign up! I have {}, and need {} more",
                        self.config.game_name,
                        self.ticket_holders.len(),
                        needed
                    ))?;
                    self.last_advertisement = Instant::now();
                }
            }
            Phase::WaitingForMore { start_time } => {
                if start_time.elapsed()
                    >= Duration::from_secs(self.config.wait_for_more_players_seconds)
                {
                    if self.ticket_holders.len() < self.config.min_players {
                        self.ticket_taker.say(&format!(
                            "Not enough ticketholders present for {}, postponing the game.",
                            self.config.game_name
                        ))?;
                        self.phase = Phase::Waiting;
                        return Ok(None);
                    }

                    self.ticket_taker.say(&format!(
                        "Starting game for {} with {} players",
                        self.config.game_name,
                        self.ticket_holders.len()
                    ))?;
                    self.phase = Phase::Countdown {
                        start_time: Instant::now(),
                    };
                }
            }
            Phase::Countdown { start_time } => {
                if start_time.elapsed() >= Duration::from_secs(self.config.countdown_seconds) {
                    let players_to_start = self.ticket_holders.clone();
                    self.phase = Phase::Waiting; // Reset for next game
                    self.ticket_holders.clear();
                    return Ok(Some(players_to_start));
                }
            }
            Phase::PostGameCooldown { start_time } => {
                if start_time.elapsed() >= self.config.ad_post_game_delay {
                    self.phase = Phase::Waiting;
                    self.last_advertisement =
                        Instant::now().sub(self.config.ad_no_players_interval);
                }
            }
        }
        Ok(None)
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
                if click
                    .object_info
                    .action
                    .contains(&self.config.ticket_taker_action)
                {
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

    fn handle_ticket_purchase(&mut self, click: &ObjectClickInfo) -> SdkResult<()> {
        match self.phase {
            Phase::Waiting | Phase::WaitingForMore { .. } => {
                if let Some(citizen_id) = self.mion_session_to_citizen.get(&click.avatar_session) {
                    let citizen_id = *citizen_id;

                    if self.ticket_holders.contains_key(&citizen_id) {
                        // Player already has a ticket, refund it.
                        self.ticket_holders.remove(&citizen_id);
                        self.client
                            .add_creditz(citizen_id, self.config.ticket_price)
                            .ok();
                        self.ticket_taker.console_message(ConsoleMessageParams {
                            message: format!(
                                "Buying back your ticket for {}, {}; click again if you want to play",
                                self.config.game_name, click.avatar_name
                            ),
                            session_id: click.avatar_session,
                            bold: false,
                            italics: false,
                            color: (0, 0, 0),
                        })?;
                    } else {
                        // Player does not have a ticket, sell one.
                        match self
                            .client
                            .sub_creditz(citizen_id, self.config.ticket_price)
                        {
                            Ok(_) => {
                                let player_info = PlayerInfo {
                                    citizen_id,
                                    session_id: click.avatar_session,
                                    name: click.avatar_name.clone(),
                                };
                                self.ticket_holders.insert(citizen_id, player_info);
                                self.ticket_taker.console_message(ConsoleMessageParams {
                                    message: format!(
                                        "{} gets a ticket for {} - {} players signed up, need at least {}",
                                        click.avatar_name,
                                        self.config.game_name,
                                        self.ticket_holders.len(),
                                        self.config.min_players
                                    ),
                                    session_id: click.avatar_session,
                                    bold: false,
                                    italics: false,
                                    color: (0, 0, 0),
                                })?;
                            }
                            Err(_) => {
                                self.ticket_taker.console_message(ConsoleMessageParams {
                                    message: format!(
                                        "Sorry {}, you don't have enough creditz to buy a ticket.",
                                        click.avatar_name
                                    ),
                                    session_id: click.avatar_session,
                                    bold: false,
                                    italics: false,
                                    color: (0, 0, 0),
                                })?;
                            }
                        }
                    }
                }
            }
            _ => {
                // Game is in progress
                self.ticket_taker.console_message(ConsoleMessageParams {
                    message: format!(
                        "{} has already started a game - try back later!",
                        self.config.game_name
                    ),
                    session_id: click.avatar_session,
                    bold: false,
                    italics: false,
                    color: (0, 0, 0),
                })?;
            }
        }
        Ok(())
    }
}
