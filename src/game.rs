use std::collections::HashMap;

use macroquad::prelude::*;

use crate::ai::AiBrain;
use crate::blob::{random_color, random_bot_name, Cell, DyingCell};
use crate::constants::*;
use crate::food::Food;
use crate::net::{ClientNet, HostNet};
use crate::protocol::*;
use crate::virus::Virus;

/// A network player tracked by the host
pub struct NetPlayer {
    pub cells: Vec<Cell>,
    pub color: Color,
    pub name: String,
    pub last_input: PlayerInput,
    pub dead: bool,
}

pub struct Game {
    // Simulation state (used in solo + host modes)
    pub player_cells: Vec<Cell>,
    pub player_color: Color,
    pub ai_blobs: Vec<Cell>,
    pub ai_brains: Vec<AiBrain>,
    pub food: Vec<Food>,
    pub ejected: Vec<crate::food::EjectedMass>,
    pub viruses: Vec<Virus>,
    pub dying_cells: Vec<DyingCell>,
    pub camera_pos: Vec2,
    pub camera_zoom: f32,
    pub game_over: bool,
    pub score: f32,
    // Networking
    pub host_net: Option<HostNet>,
    pub client_net: Option<ClientNet>,
    pub net_players: HashMap<u8, NetPlayer>,
    // Client-side render state (from host snapshots)
    pub client_state: Option<WorldState>,
    pub net_tick_accum: f32,
    pub copied_timer: f32,
    pub last_state_time: f32,
    pub host_lost: bool,
}

impl Game {
    fn base() -> Self {
        let color = Color::new(0.2, 0.6, 1.0, 1.0);
        let player_cells = vec![Cell::new(
            vec2(WORLD_SIZE / 2.0, WORLD_SIZE / 2.0),
            10.0,
            color,
            "You".to_string(),
        )];

        let mut ai_blobs = Vec::new();
        let mut ai_brains = Vec::new();
        for _ in 0..AI_COUNT {
            ai_blobs.push(Cell::new(
                vec2(
                    rand::gen_range(100.0, WORLD_SIZE - 100.0),
                    rand::gen_range(100.0, WORLD_SIZE - 100.0),
                ),
                rand::gen_range(5.0, 30.0),
                random_color(),
                random_bot_name(),
            ));
            ai_brains.push(AiBrain::new());
        }

        let food: Vec<Food> = (0..FOOD_COUNT).map(|_| Food::random()).collect();
        let viruses: Vec<Virus> = (0..VIRUS_COUNT).map(|_| Virus::random()).collect();

        Game {
            player_cells,
            player_color: color,
            ai_blobs,
            ai_brains,
            food,
            ejected: Vec::new(),
            viruses,
            dying_cells: Vec::new(),
            camera_pos: vec2(WORLD_SIZE / 2.0, WORLD_SIZE / 2.0),
            camera_zoom: 1.0,
            game_over: false,
            score: 10.0,
            host_net: None,
            client_net: None,
            client_state: None,
            net_players: HashMap::new(),
            net_tick_accum: 0.0,
            copied_timer: 0.0,
            last_state_time: 0.0,
            host_lost: false,
        }
    }

    pub fn new_solo() -> Self {
        Self::base()
    }

    pub fn new_host(host_net: HostNet) -> Self {
        let mut g = Self::base();
        g.host_net = Some(host_net);
        g
    }

    pub fn new_client(client_net: ClientNet) -> Self {
        Game {
            player_cells: Vec::new(),
            player_color: Color::new(0.2, 0.6, 1.0, 1.0),
            ai_blobs: Vec::new(),
            ai_brains: Vec::new(),
            food: Vec::new(),
            ejected: Vec::new(),
            viruses: Vec::new(),
            dying_cells: Vec::new(),
            camera_pos: vec2(WORLD_SIZE / 2.0, WORLD_SIZE / 2.0),
            camera_zoom: 1.0,
            game_over: false,
            score: 0.0,
            host_net: None,
            client_net: Some(client_net),
            client_state: None,
            net_players: HashMap::new(),
            net_tick_accum: 0.0,
            copied_timer: 0.0,
            last_state_time: 0.0,
            host_lost: false,
        }
    }
}
