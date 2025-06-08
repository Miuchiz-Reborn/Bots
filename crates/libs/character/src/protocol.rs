use serde::{Deserialize, Serialize};

pub type UserId = u32;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StatBar {
    value_u32: u32, // Internally range 0..=0x7FFFFFFF
}

impl StatBar {
    pub fn from_u32(value_u32: u32) -> Self {
        Self { value_u32 }
    }

    pub fn from_f32(value_f32: f32) -> Self {
        // Clamp the value to the range 0..=1.0
        let clamped_value = value_f32.clamp(0.0, 1.0);
        Self {
            value_u32: (clamped_value * 0x7FFFFFFF as f32) as u32,
        }
    }

    pub fn to_f32(&self) -> f32 {
        // Clamp the value to the range 0..=1.0
        let clamped_value = self.value_u32 as f32 / 0x7FFFFFFF as f32;
        clamped_value.clamp(0.0, 1.0)
    }

    pub fn to_u32(&self) -> u32 {
        self.value_u32
    }
}

/// A request sent from a client to the server.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Request {
    GetCreditz(UserId),
    SetCreditz(UserId, u32),
    AddCreditz(UserId, u32),
    SubtractCreditz(UserId, u32),
    GetHappiness(UserId),
    SetHappiness(UserId, f32),
    GetBoredom(UserId),
    SetBoredom(UserId, f32),
    GetHunger(UserId),
    SetHunger(UserId, f32),
}

/// A top-level message sent from the server to clients.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerMessage {
    Response(Response),
    Notification(Notification),
}

/// A direct response to a specific client Request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Response {
    Creditz(u32),
    Happiness(StatBar),
    Boredom(StatBar),
    Hunger(StatBar),
    Success,
    Error(String),
}

/// A notification broadcast from the server to all connected clients.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Notification {
    CreditzChanged { user_id: UserId, new_value: u32 },
    HappinessChanged { user_id: UserId, new_value: StatBar },
    BoredomChanged { user_id: UserId, new_value: StatBar },
    HungerChanged { user_id: UserId, new_value: StatBar },
}
