use macroquad::prelude::Vec2;
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

pub const GAME_ALPN: &[u8] = b"phage/1";

/// Sent from client to host each frame
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug)]
pub struct PlayerInput {
    pub direction: [f32; 2], // target world position relative to player center
    pub split: bool,
    pub eject: bool,
}

/// A single cell in the world snapshot
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug)]
pub struct CellState {
    pub pos: [f32; 2],
    pub mass: f32,
    pub color: [f32; 4],
    pub name: String,
    pub owner_id: u8, // 0 = this client's player, 1+ = other players / AI
}

/// A food pellet
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug)]
pub struct FoodState {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

/// An ejected mass blob
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug)]
pub struct EjectedState {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

/// A virus
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug)]
pub struct VirusState {
    pub pos: [f32; 2],
}

/// Leaderboard entry
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug)]
pub struct LeaderEntry {
    pub name: String,
    pub mass: u32,
    pub is_you: bool,
}

/// Full world state broadcast from host to clients
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug)]
pub struct WorldState {
    pub cells: Vec<CellState>,
    pub food: Vec<FoodState>,
    pub ejected: Vec<EjectedState>,
    pub viruses: Vec<VirusState>,
    pub leaderboard: Vec<LeaderEntry>,
    pub your_score: u32,
    pub game_over: bool,
}

/// Messages from host to client
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug)]
pub enum HostMessage {
    /// Your player ID assignment
    Welcome { player_id: u8 },
    /// World state update
    State(WorldState),
}

/// Messages from client to host
#[derive(Serialize, Deserialize, Encode, Decode, Clone, Debug)]
pub enum ClientMessage {
    Input(PlayerInput),
}

/// Helpers
impl PlayerInput {
    pub fn to_vec2(&self) -> Vec2 {
        Vec2::new(self.direction[0], self.direction[1])
    }
}
