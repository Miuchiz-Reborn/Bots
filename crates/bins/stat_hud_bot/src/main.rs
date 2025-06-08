use std::collections::HashMap;

use aw_sdk::{
    AvatarAddInfo, AvatarDeleteInfo, AwEvent, AwInstance, HudCreateParams, HudElementFlags,
    HudOrigin, HudType, LoginParams, SdkResult, StateChangeParams,
};
use character::{CharacterClient, Notification, StatBar};

const HUD_FRAME_ELEMENT_ID: u32 = 1;
const HUD_CREDITZ_ELEMENT_ID: u32 = 2;
const HUD_HAPPINESS_ELEMENT_ID: u32 = 3;
const HUD_HUNGER_ELEMENT_ID: u32 = 4;
const HUD_BOREDOM_ELEMENT_ID: u32 = 5;

const GREEN_THRESHOLD: f32 = 0.5;
const YELLOW_THRESHOLD: f32 = 0.25;

const GREEN_COLOR: (u8, u8, u8) = (0x00, 0xFF, 0x00);
const YELLOW_COLOR: (u8, u8, u8) = (0xFF, 0xFF, 0x00);
const RED_COLOR: (u8, u8, u8) = (0xFF, 0x00, 0x00);

#[derive(Debug, Clone)]
struct PlayerHudState {
    creditz: u32,
    happiness: StatBar,
    hunger: StatBar,
    boredom: StatBar,
}

struct StatsHudBot {
    instance: AwInstance,
    client: CharacterClient,
    // Maps citizen ID to their current HUD state.
    hud_states: HashMap<u32, PlayerHudState>,
    // Maps AW session ID to citizen ID.
    session_to_citizen: HashMap<u32, u32>,
    // Maps citizen ID to AW session ID for quick lookups.
    citizen_to_session: HashMap<u32, u32>,
}

impl StatsHudBot {
    fn new() -> Self {
        Self {
            instance: AwInstance::new("127.0.0.1", 6670).unwrap(),
            client: CharacterClient::connect("127.0.0.1:6675").unwrap(),
            hud_states: HashMap::new(),
            session_to_citizen: HashMap::new(),
            citizen_to_session: HashMap::new(),
        }
    }

    fn run(&mut self) -> SdkResult<()> {
        self.instance.login(LoginParams::Bot {
            name: "Stats Bot".to_string(),
            owner_id: 1,
            privilege_password: "pass".to_string(),
            application: "Stats Bot".to_string(),
        })?;
        self.instance.enter("MION", true)?;
        self.instance.state_change(StateChangeParams {
            north: 0,
            height: 0,
            west: 0,
            rotation: 0,
            gesture: 0,
            av_type: 0,
            av_state: 0,
        })?;

        loop {
            let sdk_events = self.instance.tick();
            for event in &sdk_events {
                match event {
                    AwEvent::AvatarAdd(avatar_add) => {
                        self.on_avatar_add(avatar_add)?;
                    }
                    AwEvent::AvatarDelete(avatar_delete) => {
                        self.on_avatar_delete(avatar_delete)?;
                    }
                    _ => {}
                }
            }

            match self.client.check_events() {
                Ok(notifications) => {
                    for notif in notifications {
                        self.on_notification(notif)?;
                    }
                }
                Err(e) => {
                    println!("[Error checking server events: {}]", e);
                }
            }
        }
    }

    fn on_notification(&mut self, notification: Notification) -> SdkResult<()> {
        let citizen_id = match notification {
            Notification::CreditzChanged { user_id, new_value } => {
                let cid = user_id as u32;
                if let Some(state) = self.hud_states.get_mut(&cid) {
                    state.creditz = new_value;
                }
                cid
            }
            Notification::HappinessChanged { user_id, new_value } => {
                let cid = user_id as u32;
                if let Some(state) = self.hud_states.get_mut(&cid) {
                    state.happiness = new_value;
                }
                cid
            }
            Notification::HungerChanged { user_id, new_value } => {
                let cid = user_id as u32;
                if let Some(state) = self.hud_states.get_mut(&cid) {
                    state.hunger = new_value;
                }
                cid
            }
            Notification::BoredomChanged { user_id, new_value } => {
                let cid = user_id as u32;
                if let Some(state) = self.hud_states.get_mut(&cid) {
                    state.boredom = new_value;
                }
                cid
            }
        };

        if let Some(session_id) = self.citizen_to_session.get(&citizen_id).cloned() {
            self.render_hud_for(session_id)?;
        }

        Ok(())
    }

    fn on_avatar_add(&mut self, avatar_add: &AvatarAddInfo) -> SdkResult<()> {
        let session_id = avatar_add.session_id;
        let Some(citizen_id) = avatar_add.citizen_id else {
            return Ok(());
        };

        let creditz = self.client.get_creditz(citizen_id).unwrap_or(0);
        let happiness = self.client.get_happiness(citizen_id).unwrap_or(0.0);
        let hunger = self.client.get_hunger(citizen_id).unwrap_or(0.0);
        let boredom = self.client.get_boredom(citizen_id).unwrap_or(0.0);

        let player_hud_state = PlayerHudState {
            creditz,
            happiness: StatBar::from_f32(happiness),
            hunger: StatBar::from_f32(hunger),
            boredom: StatBar::from_f32(boredom),
        };

        self.hud_states.insert(citizen_id, player_hud_state);
        self.session_to_citizen.insert(session_id, citizen_id);
        self.citizen_to_session.insert(citizen_id, session_id);

        self.render_hud_for(session_id)?;
        Ok(())
    }

    fn on_avatar_delete(&mut self, avatar_delete: &AvatarDeleteInfo) -> SdkResult<()> {
        let session_id = avatar_delete.session_id;
        if let Some(citizen_id) = self.session_to_citizen.remove(&session_id) {
            self.citizen_to_session.remove(&citizen_id);
            self.hud_states.remove(&citizen_id);
        }

        // No need to destroy the HUD elements for this, since the user is gone

        Ok(())
    }

    fn render_hud_for(&mut self, session_id: u32) -> SdkResult<()> {
        let Some(citizen_id) = self.session_to_citizen.get(&session_id).cloned() else {
            return Ok(());
        };
        let Some(player_hud_state) = self.hud_states.get(&citizen_id).cloned() else {
            return Ok(());
        };

        self.instance.hud_create(HudCreateParams {
            element_type: HudType::Image {
                texture_name: "hud_bar_frame_cloe.png".to_string(),
                texture_offset_x: 0,
                texture_offset_y: 0,
            },
            element_id: HUD_FRAME_ELEMENT_ID,
            user_session: session_id,
            element_origin: HudOrigin::TopLeft,
            element_opacity: 0.9,
            element_x: 0,
            element_y: 0,
            element_z: 0,
            element_flags: vec![],
            element_color: (255, 255, 255),
            element_size_x: 256,
            element_size_y: 128,
            element_size_z: 0,
        })?;

        let creditz_formatted = player_hud_state
            .creditz
            .to_string()
            .chars()
            .rev()
            .collect::<Vec<_>>()
            .chunks(3)
            .map(|chunk| chunk.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join(",")
            .chars()
            .rev()
            .collect::<String>();
        let creditz_text = format!("M{}", creditz_formatted);
        self.instance.hud_create(HudCreateParams {
            element_type: HudType::Text {
                element_text: creditz_text,
            },
            element_id: HUD_CREDITZ_ELEMENT_ID,
            user_session: session_id,
            element_origin: HudOrigin::TopLeft,
            element_opacity: 0.8,
            element_x: 0,
            element_y: 90,
            element_z: 0,
            element_flags: vec![],
            element_color: (0x91, 0xF0, 0x8C),
            element_size_x: 255,
            element_size_y: 55,
            element_size_z: 0,
        })?;

        let happiness_color = hud_stat_color(player_hud_state.happiness.to_f32());
        self.instance.hud_create(HudCreateParams {
            element_type: HudType::Image {
                texture_name: "hud_bar.png".to_string(),
                texture_offset_x: 0,
                texture_offset_y: 0,
            },
            element_id: HUD_HAPPINESS_ELEMENT_ID,
            user_session: session_id,
            element_origin: HudOrigin::TopLeft,
            element_opacity: 0.9,
            element_x: 0,
            element_y: 0,
            element_z: 0,
            element_flags: vec![HudElementFlags::Transition, HudElementFlags::Additive],
            element_color: happiness_color,
            element_size_x: hud_stat_size_x(player_hud_state.happiness.to_f32()),
            element_size_y: 32,
            element_size_z: 0,
        })?;

        let hunger_color = hud_stat_color(player_hud_state.hunger.to_f32());
        self.instance.hud_create(HudCreateParams {
            element_type: HudType::Image {
                texture_name: "hud_bar.png".to_string(),
                texture_offset_x: 0,
                texture_offset_y: 0,
            },
            element_id: HUD_HUNGER_ELEMENT_ID,
            user_session: session_id,
            element_origin: HudOrigin::TopLeft,
            element_opacity: 0.8,
            element_x: 0,
            element_y: 32,
            element_z: 0,
            element_flags: vec![HudElementFlags::Transition, HudElementFlags::Additive],
            element_color: hunger_color,
            element_size_x: hud_stat_size_x(player_hud_state.hunger.to_f32()),
            element_size_y: 32,
            element_size_z: 0,
        })?;

        let boredom_color = hud_stat_color(player_hud_state.boredom.to_f32());
        self.instance.hud_create(HudCreateParams {
            element_type: HudType::Image {
                texture_name: "hud_bar.png".to_string(),
                texture_offset_x: 0,
                texture_offset_y: 0,
            },
            element_id: HUD_BOREDOM_ELEMENT_ID,
            user_session: session_id,
            element_origin: HudOrigin::TopLeft,
            element_opacity: 0.8,
            element_x: 0,
            element_y: 64,
            element_z: 0,
            element_flags: vec![HudElementFlags::Transition, HudElementFlags::Additive],
            element_color: boredom_color,
            element_size_x: hud_stat_size_x(player_hud_state.boredom.to_f32()),
            element_size_y: 32,
            element_size_z: 0,
        })?;

        Ok(())
    }
}

fn hud_stat_color(stat: f32) -> (u8, u8, u8) {
    if stat >= GREEN_THRESHOLD {
        GREEN_COLOR
    } else if stat >= YELLOW_THRESHOLD {
        YELLOW_COLOR
    } else {
        RED_COLOR
    }
}

fn hud_stat_size_x(stat: f32) -> u32 {
    let min_size = 32.0;
    let max_size = 256.0;
    let scale_factor = (max_size - min_size);
    (min_size + (stat * scale_factor)) as u32
}

fn main() {
    loop {
        let mut bot = StatsHudBot::new();
        let result = bot.run();
        match result {
            Ok(_) => {}
            Err(why) => {
                println!("Error: {:?}", why);
            }
        }
    }
}
