use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use aw_sdk::{
    AwEvent, AwInstance, ConsoleMessageParams, LoginParams, ObjectBumpInfo, SdkError, SdkResult,
    StateChangeParams, TeleportParams, cell_from_cm, sector_from_cell,
};
use character::CharacterClient;
use game_manager::{GameConfig, GameManager, PlayerInfo};

// =================================================================================================
//                                         CONFIGURATION
// =================================================================================================

// --- Game Settings ---
const GAME_DURATION_SECONDS: u64 = 360; // 6 minutes
const POST_GAME_SECONDS: u64 = 10;
const FINAL_PRIZE_CREDITZ: u32 = 60;

// =================================================================================================
//                                          STATE
// =================================================================================================

#[derive(Debug, Clone)]
struct PlayerInGameInfo {
    citizen_id: u32,
    name: String,
    session_id: u32,
    next_checkpoint: u32,
}

#[derive(Debug, Clone)]
enum GamePhase {
    NotStarted,
    Teleporting {
        players: HashMap<u32, PlayerInGameInfo>,
    },
    InProgress {
        start_time: Instant,
        players: HashMap<u32, PlayerInGameInfo>,
        thirty_second_warning_given: bool,
    },
    Ending {
        end_time: Instant,
        players: HashMap<u32, PlayerInGameInfo>,
    },
}

impl Default for GamePhase {
    fn default() -> Self {
        GamePhase::NotStarted
    }
}

pub struct ObstacleBot {
    config: ObstacleBotConfig,
    game_manager: GameManager,
    game_world_instance: AwInstance,
    client: CharacterClient,
    game_phase: GamePhase,
    forest_session_to_citizen: HashMap<u32, u32>,
}

// =================================================================================================
//                                        IMPLEMENTATION
// =================================================================================================

impl ObstacleBot {
    pub fn new(config: ObstacleBotConfig) -> Result<Self, InitError> {
        let game_config = GameConfig {
            game_name: config.game_name.clone(),
            tagline: config.tagline.clone(),
            ticket_price: config.ticket_price,
            min_players: config.min_players as usize,
            wait_for_more_players_seconds: 60,
            countdown_seconds: 10,
            ticket_world_name: config.ticket_world_name.clone(),
            game_world_name: config.game_world_name.clone(),
            ticket_taker_pos: config.ticket_taker_pos,
            game_spawn_pos: config.game_spawn_pos,
            mion_return_spawn_pos: config.mion_return_spawn_pos,
            ticket_taker_action: config.ticket_taker_action.clone(),
            ad_no_players_interval: config.ad_no_players_interval,
            ad_waiting_interval: config.ad_waiting_interval,
            ad_post_game_delay: config.ad_post_game_delay,
        };

        let character_addr = format!("{}:{}", config.character_host, config.character_port);

        let game_manager =
            GameManager::new(&config.host, config.port, &character_addr, game_config)
                .map_err(InitError::GameManager)?;
        let game_world_instance = AwInstance::new(&config.host, config.port)
            .map_err(|e| InitError::GameInstance(e.to_string()))?;
        let client = CharacterClient::connect(&character_addr)
            .map_err(|e| InitError::CharacterClient(e.to_string()))?;

        Ok(Self {
            config,
            game_manager,
            game_world_instance,
            client,
            game_phase: GamePhase::default(),
            forest_session_to_citizen: HashMap::new(),
        })
    }

    pub fn run(&mut self) -> SdkResult<()> {
        self.game_manager
            .login(self.config.owner_id, &self.config.privilege_password)?;

        self.game_world_instance.login(LoginParams::Bot {
            name: "ObstacleBot".to_string(),
            owner_id: self.config.owner_id,
            privilege_password: self.config.privilege_password.clone(),
            application: "ObstacleBot".to_string(),
        })?;
        self.game_world_instance
            .enter(&self.config.game_world_name, true)?;
        self.game_world_instance.state_change(StateChangeParams {
            west: 0,
            height: 0,
            north: 0,
            rotation: 0,
            gesture: 0,
            av_type: 20,
            av_state: 0,
        })?;

        loop {
            if let Some(players) = self.game_manager.tick()? {
                self.start_game(players)?;
            }

            self.update_game_state()?;

            let mf_events = self.game_world_instance.tick();
            for event in mf_events {
                self.handle_game_world_instance_event(&event)?;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn start_game(&mut self, players: HashMap<u32, PlayerInfo>) -> SdkResult<()> {
        self.game_manager.teleport_to_game(&players)?;
        let in_game_players = players
            .into_iter()
            .map(|(id, info)| {
                (
                    id,
                    PlayerInGameInfo {
                        citizen_id: id,
                        name: info.name,
                        session_id: 0,
                        next_checkpoint: 0,
                    },
                )
            })
            .collect();
        self.game_phase = GamePhase::Teleporting {
            players: in_game_players,
        };
        Ok(())
    }

    fn update_game_state(&mut self) -> SdkResult<()> {
        let current_phase = self.game_phase.clone();
        match current_phase {
            GamePhase::NotStarted => {}
            GamePhase::Teleporting { players } => {
                let all_arrived = players.values().all(|p| p.session_id != 0);
                if all_arrived {
                    for message in self.config.welcome_messages.clone() {
                        self.broadcast_console_message_ingame(&players, &message)?;
                    }

                    self.game_phase = GamePhase::InProgress {
                        start_time: Instant::now(),
                        players,
                        thirty_second_warning_given: false,
                    };
                }
            }
            GamePhase::InProgress {
                start_time,
                players,
                thirty_second_warning_given,
            } => {
                let elapsed = start_time.elapsed();
                if elapsed >= Duration::from_secs(GAME_DURATION_SECONDS) {
                    self.broadcast_console_message_ingame(
                        &players,
                        "You ran out of time :(  Better luck next time :-)!",
                    )?;
                    self.end_game(players)?;
                } else if !thirty_second_warning_given
                    && (Duration::from_secs(GAME_DURATION_SECONDS) - elapsed)
                        <= Duration::from_secs(30)
                {
                    self.broadcast_console_message_ingame(
                        &players,
                        &self.config.thirty_second_warning_message.clone(),
                    )?;
                    if let GamePhase::InProgress {
                        thirty_second_warning_given: given,
                        ..
                    } = &mut self.game_phase
                    {
                        *given = true;
                    }
                }
            }
            GamePhase::Ending { end_time, players } => {
                if end_time.elapsed() >= Duration::from_secs(POST_GAME_SECONDS) {
                    for player in players.values() {
                        self.game_world_instance.teleport(TeleportParams {
                            session_id: player.session_id,
                            world: self.config.ticket_world_name.clone(),
                            west: self.config.mion_return_spawn_pos.0,
                            height: self.config.mion_return_spawn_pos.1,
                            north: self.config.mion_return_spawn_pos.2,
                            rotation: self.config.mion_return_spawn_pos.3,
                            warp: false,
                        })?;
                    }
                    self.game_phase = GamePhase::NotStarted;
                    self.game_manager.game_is_over()?;
                }
            }
        }
        Ok(())
    }

    fn handle_game_world_instance_event(&mut self, event: &AwEvent) -> SdkResult<()> {
        match event {
            AwEvent::AvatarAdd(avatar_add) => {
                if let Some(citizen_id) = avatar_add.citizen_id {
                    self.forest_session_to_citizen
                        .insert(avatar_add.session_id, citizen_id);
                    match &mut self.game_phase {
                        GamePhase::InProgress { players, .. }
                        | GamePhase::Teleporting { players } => {
                            if let Some(player) = players.get_mut(&citizen_id) {
                                player.session_id = avatar_add.session_id;
                            }
                        }
                        _ => {}
                    }
                }
            }
            AwEvent::AvatarDelete(avatar_delete) => {
                self.forest_session_to_citizen
                    .remove(&avatar_delete.session_id);
            }
            AwEvent::ObjectBump(bump) => {
                // println!("bump: {:?}", bump);
                if let Some(citizen_id) = self
                    .forest_session_to_citizen
                    .get(&bump.avatar_session)
                    .copied()
                {
                    // println!("citizen_id: {:?}", citizen_id);
                    let mut player =
                        if let GamePhase::InProgress { players, .. } = &mut self.game_phase {
                            players.remove(&citizen_id)
                        } else {
                            None
                        };

                    // println!("player: {:?}", player);
                    let mut game_over = false;
                    if let Some(ref mut p) = player {
                        game_over = self.handle_checkpoint(p, bump)?;
                    }

                    if let Some(p) = player {
                        if let GamePhase::InProgress { players, .. } = &mut self.game_phase {
                            players.insert(citizen_id, p);
                        }
                    }

                    if game_over {
                        if let GamePhase::InProgress { players, .. } = self.game_phase.clone() {
                            self.end_game(players)?;
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

    fn handle_checkpoint(
        &mut self,
        player: &mut PlayerInGameInfo,
        bump: &ObjectBumpInfo,
    ) -> SdkResult<bool> {
        // println!("bump: {:?}", bump);
        let keyword = format!("~{}", self.config.bump_keyword);
        if let Some(start) = bump.object_info.action.find(&keyword) {
            // println!("start: {:?}", start);
            let num_str: String = bump.object_info.action[start + keyword.len()..]
                .chars()
                .take_while(|c| c.is_digit(10))
                .collect();
            // println!("num_str: {:?}", num_str);
            if let Ok(checkpoint_num) = num_str.parse::<u32>() {
                // println!("checkpoint_num: {:?}", checkpoint_num);
                if checkpoint_num == player.next_checkpoint {
                    // println!("checkpoint_num == player.next_checkpoint");
                    self.game_world_instance
                        .console_message(ConsoleMessageParams {
                            message: format!(
                                "You have passed checkpoint #{} ({}) of {}!",
                                checkpoint_num,
                                checkpoint_num,
                                self.final_checkpoint_index()
                            ),
                            session_id: player.session_id,
                            bold: false,
                            italics: false,
                            color: (0, 0, 0),
                        })?;
                    player.next_checkpoint += 1;

                    if player.next_checkpoint > self.final_checkpoint_index() {
                        return Ok(true);
                    }
                } else {
                    self.game_world_instance
                        .console_message(ConsoleMessageParams {
                            message: format!(
                                "You missed number {}. You need to find it first, then come back here.",
                                player.next_checkpoint
                            ),
                            session_id: player.session_id,
                            bold: false,
                            italics: false,
                            color: (0, 0, 0),
                        })?;
                }
            }
        }
        Ok(false)
    }

    fn final_checkpoint_index(&self) -> u32 {
        self.config.total_checkpoints - 1
    }

    fn end_game(&mut self, players: HashMap<u32, PlayerInGameInfo>) -> SdkResult<()> {
        let winner = players
            .values()
            .find(|p| p.next_checkpoint > self.final_checkpoint_index());

        // println!("winner: {:?}", winner);
        // println!("game_phase: {:?}", self.game_phase);
        // println!("players: {:?}", players);

        if let Some(winner) = winner {
            if let GamePhase::InProgress { start_time, .. } = self.game_phase {
                let time_to_win = start_time.elapsed();
                self.broadcast_console_message_ingame(
                    &players,
                    &(self.config.win_game_message)(&winner.name, time_to_win.as_secs()),
                )?;
            }

            // Update winner board
            let sector_x = sector_from_cell(cell_from_cm(self.config.ticket_taker_pos.0));
            let sector_z = sector_from_cell(cell_from_cm(self.config.ticket_taker_pos.2));
            // println!("sector_x: {:?}", sector_x);
            // println!("sector_z: {:?}", sector_z);
            if let SdkResult::Ok(result) = self.game_manager.query(sector_x, sector_z) {
                // println!("result: {:?}", result);
                for object in result.objects {
                    // println!("object: {:?}", object);
                    if object
                        .action
                        .contains(&format!("~{}~", &self.config.sign_keyword))
                    {
                        // println!("object.action: {:?}", object.action);
                        let mut new_object = object.clone();
                        new_object.description =
                            format!("{}\nLast winner: {}", self.config.game_name, winner.name);
                        self.game_manager.object_change(new_object)?;
                    }
                }
            }
        }

        self.broadcast_console_message_ingame(&players, "Here are the final scores:")?;
        for player in players.values() {
            let score = if player.next_checkpoint > self.final_checkpoint_index() {
                FINAL_PRIZE_CREDITZ
            } else {
                player.next_checkpoint
            };

            let message = format!("{} collected {} credits", player.name, score);
            self.broadcast_console_message_ingame(&players, &message)?;
            self.client.add_creditz(player.citizen_id, score).ok();
            if let Ok(happiness) = self.client.get_happiness(player.citizen_id) {
                self.client
                    .set_happiness(player.citizen_id, happiness + 0.1)
                    .ok();
            }
            if let Ok(boredom) = self.client.get_boredom(player.citizen_id) {
                self.client
                    .set_boredom(player.citizen_id, boredom + 0.25)
                    .ok();
            }
        }

        self.broadcast_console_message_ingame(
            &players,
            "Thanks for playing!  I'll send you home in a few seconds :)",
        )?;

        self.game_phase = GamePhase::Ending {
            end_time: Instant::now(),
            players,
        };

        Ok(())
    }

    fn broadcast_console_message_ingame(
        &mut self,
        players: &HashMap<u32, PlayerInGameInfo>,
        message: &str,
    ) -> SdkResult<()> {
        for player in players.values() {
            if player.session_id != 0 {
                self.game_world_instance
                    .console_message(ConsoleMessageParams {
                        message: message.to_string(),
                        session_id: player.session_id,
                        bold: false,
                        italics: false,
                        color: (0, 0, 0),
                    })?;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum InitError {
    GameManager(String),
    GameInstance(String),
    CharacterClient(String),
}

pub struct ObstacleBotConfig {
    pub host: String,
    pub port: u16,
    pub character_host: String,
    pub character_port: u16,

    pub owner_id: u32,
    pub privilege_password: String,

    pub game_name: String,
    pub tagline: Option<String>,
    pub ticket_price: u32,
    pub min_players: u32,
    pub ticket_world_name: String,
    pub game_world_name: String,
    pub ticket_taker_pos: (i32, i32, i32),
    pub game_spawn_pos: (i32, i32, i32, i32),
    pub mion_return_spawn_pos: (i32, i32, i32, i32),
    pub total_checkpoints: u32,
    pub bump_keyword: String,        // Like "PawzRacer"
    pub sign_keyword: String,        // Like "WinnerMagicForest"
    pub ticket_taker_action: String, // Like "~TicketTaker=MagicForest~"

    pub welcome_messages: Vec<String>,
    pub win_game_message: Box<dyn Fn(&str /* winner name */, u64 /* seconds */) -> String>,
    pub thirty_second_warning_message: String,
    pub ad_no_players_interval: Duration,
    pub ad_waiting_interval: Duration,
    pub ad_post_game_delay: Duration,
}
