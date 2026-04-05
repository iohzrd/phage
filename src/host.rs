use macroquad::prelude::*;

use crate::ai::AiBrain;
use crate::blob::{center_of_mass, merge_cells, push_apart, random_color, total_mass, Cell};
use crate::constants::*;
use crate::food::{EjectedMass, Food};
use crate::game::{Game, NetPlayer};
use crate::protocol::*;
use crate::virus::Virus;

const NET_TICK_RATE: f32 = 1.0 / 20.0;

impl Game {
    /// Process network player inputs, simulate net players, broadcast state
    pub fn update_host(&mut self, dt: f32) {
        let host_needs_respawn = if let Some(ref host_net) = self.host_net {
            // Receive inputs from clients
            while let Ok((player_id, input)) = host_net.input_rx.try_recv() {
                if !self.net_players.contains_key(&player_id) {
                    let color = random_color();
                    let name = format!("Player {}", player_id);
                    let spawn = vec2(
                        rand::gen_range(100.0, WORLD_SIZE - 100.0),
                        rand::gen_range(100.0, WORLD_SIZE - 100.0),
                    );
                    self.net_players.insert(player_id, NetPlayer {
                        cells: vec![Cell::new(spawn, 10.0, color, name.clone())],
                        color,
                        name,
                        last_input: input.clone(),
                        dead: false,
                    });
                    if !self.ai_blobs.is_empty() {
                        self.ai_blobs.pop();
                        self.ai_brains.pop();
                    }
                }
                if let Some(np) = self.net_players.get_mut(&player_id) {
                    np.last_input = input;
                }
            }

            // Update network player cells
            for np in self.net_players.values_mut() {
                if np.dead {
                    continue;
                }
                let target = vec2(np.last_input.direction[0], np.last_input.direction[1]);

                for cell in &mut np.cells {
                    cell.update_velocity(dt);
                    let dir = target - cell.pos;
                    let dist = dir.length();
                    if dist > 5.0 {
                        let move_dir = dir.normalize();
                        let speed = cell.speed();
                        cell.pos += move_dir * speed * dt;
                    }
                    cell.clamp_to_world();
                    cell.merge_timer = (cell.merge_timer - dt).max(0.0);
                    cell.apply_decay(dt);
                }

                push_apart(&mut np.cells, dt);
                merge_cells(&mut np.cells);

                // Split
                if np.last_input.split && np.cells.len() < MAX_CELLS {
                    let mut new_cells = Vec::new();
                    let current_count = np.cells.len();
                    for cell in &mut np.cells {
                        if current_count + new_cells.len() >= MAX_CELLS {
                            break;
                        }
                        if cell.mass < SPLIT_MIN_MASS {
                            continue;
                        }
                        let half = cell.mass / 2.0;
                        cell.mass = half;
                        let dir = (target - cell.pos).normalize_or_zero();
                        let merge_time = MERGE_TIME_BASE + half * MERGE_TIME_SCALE;
                        let mut new = Cell::new(cell.pos, half, cell.color, cell.name.clone());
                        new.velocity = dir * SPLIT_LAUNCH_SPEED;
                        new.merge_timer = merge_time;
                        cell.merge_timer = merge_time;
                        new_cells.push(new);
                    }
                    np.cells.extend(new_cells);
                }

                // Eject
                if np.last_input.eject {
                    for cell in &mut np.cells {
                        if cell.mass < EJECT_MIN_MASS {
                            continue;
                        }
                        cell.mass -= EJECT_MASS_COST;
                        let dir = (target - cell.pos).normalize_or_zero();
                        let spawn_pos = cell.pos + dir * (cell.radius() + 10.0);
                        self.ejected.push(EjectedMass::new(spawn_pos, dir, cell.color));
                    }
                }

                // Net player cells eat food
                for cell in &mut np.cells {
                    let r = cell.radius();
                    let before = self.food.len();
                    self.food.retain(|f| cell.pos.distance(f.pos) >= r);
                    let eaten = before - self.food.len();
                    cell.mass += eaten as f32 * FOOD_MASS;
                }

                // Net player cells eat ejected mass
                for cell in &mut np.cells {
                    let r = cell.radius();
                    let before = self.ejected.len();
                    self.ejected.retain(|e| cell.pos.distance(e.pos) >= r);
                    let eaten = before - self.ejected.len();
                    cell.mass += eaten as f32 * EJECT_MASS_PELLET;
                }

                if np.cells.is_empty() {
                    np.dead = true;
                }
            }

            // Net players eat AI / AI eats net players
            for np in self.net_players.values_mut() {
                if np.dead {
                    continue;
                }
                let mut eaten_ai = Vec::new();
                for (i, ai) in self.ai_blobs.iter().enumerate() {
                    for cell in &mut np.cells {
                        let dist = cell.pos.distance(ai.pos);
                        if dist < cell.radius() && cell.can_eat_mass(ai.mass) {
                            cell.mass += ai.mass * 0.8;
                            if !eaten_ai.contains(&i) {
                                eaten_ai.push(i);
                            }
                        }
                    }
                }
                eaten_ai.sort_unstable();
                for i in eaten_ai.into_iter().rev() {
                    self.ai_blobs.remove(i);
                    self.ai_brains.remove(i);
                }

                let mut eaten_cells = Vec::new();
                for ai in &mut self.ai_blobs {
                    for (ci, cell) in np.cells.iter().enumerate() {
                        let dist = ai.pos.distance(cell.pos);
                        if dist < ai.radius() && ai.can_eat_mass(cell.mass) {
                            ai.mass += cell.mass * 0.8;
                            if !eaten_cells.contains(&ci) {
                                eaten_cells.push(ci);
                            }
                        }
                    }
                }
                eaten_cells.sort_unstable();
                for i in eaten_cells.into_iter().rev() {
                    np.cells.remove(i);
                }
                if np.cells.is_empty() {
                    np.dead = true;
                }
            }

            // Net players eat host player / host player eats net players
            let mut host_needs_respawn_inner = false;
            for np in self.net_players.values_mut() {
                if np.dead {
                    continue;
                }
                let mut eaten_host = Vec::new();
                for (hi, hcell) in self.player_cells.iter().enumerate() {
                    for ncell in &mut np.cells {
                        let dist = ncell.pos.distance(hcell.pos);
                        if dist < ncell.radius() && ncell.can_eat_mass(hcell.mass) {
                            ncell.mass += hcell.mass * 0.8;
                            if !eaten_host.contains(&hi) {
                                eaten_host.push(hi);
                            }
                        }
                    }
                }
                eaten_host.sort_unstable();
                for i in eaten_host.into_iter().rev() {
                    self.player_cells.remove(i);
                }
                if self.player_cells.is_empty() {
                    host_needs_respawn_inner = true;
                }

                let mut eaten_net = Vec::new();
                for (ni, ncell) in np.cells.iter().enumerate() {
                    for hcell in &mut self.player_cells {
                        let dist = hcell.pos.distance(ncell.pos);
                        if dist < hcell.radius() && hcell.can_eat_mass(ncell.mass) {
                            hcell.mass += ncell.mass * 0.8;
                            if !eaten_net.contains(&ni) {
                                eaten_net.push(ni);
                            }
                        }
                    }
                }
                eaten_net.sort_unstable();
                for i in eaten_net.into_iter().rev() {
                    np.cells.remove(i);
                }
                if np.cells.is_empty() {
                    np.dead = true;
                }
            }

            // Respawn dead net players
            for np in self.net_players.values_mut() {
                if np.dead {
                    let spawn = vec2(
                        rand::gen_range(100.0, WORLD_SIZE - 100.0),
                        rand::gen_range(100.0, WORLD_SIZE - 100.0),
                    );
                    np.cells = vec![Cell::new(spawn, 10.0, np.color, np.name.clone())];
                    np.dead = false;
                }
            }

            // Broadcast state
            self.net_tick_accum += dt;
            if self.net_tick_accum >= NET_TICK_RATE {
                self.net_tick_accum = 0.0;
                let state = self.snapshot_world_state();
                let _ = host_net.state_tx.send(state);
            }

            host_needs_respawn_inner
        } else {
            false
        };

        if host_needs_respawn {
            if self.host_net.is_some() {
                self.respawn_host_player();
            } else {
                self.game_over = true;
            }
        }
    }

    /// Client update: send inputs, receive state
    pub fn update_client(&mut self) {
        let dt = get_frame_time().min(0.05);

        if let Some(ref client_net) = self.client_net {
            let target_world = self.screen_to_world(mouse_position().into());
            let target_world = if let Some(touch) = touches().first() {
                self.screen_to_world(vec2(touch.position.x, touch.position.y))
            } else {
                target_world
            };

            let input = PlayerInput {
                direction: [target_world.x, target_world.y],
                split: is_key_pressed(KeyCode::Space),
                eject: is_key_pressed(KeyCode::W),
            };
            let _ = client_net.input_tx.send(input);

            let mut got_state = false;
            while let Ok(msg) = client_net.state_rx.try_recv() {
                match msg {
                    HostMessage::Welcome { player_id } => {
                        eprintln!("[client] received welcome, my player_id={}", player_id);
                        self.my_player_id = Some(player_id);
                    }
                    HostMessage::State(state) => {
                        self.prev_state = self.client_state.take();
                        self.client_state = Some(state);
                        self.state_lerp_t = 0.0;
                        got_state = true;
                    }
                }
            }
            if got_state {
                self.last_state_time = 0.0;
            } else {
                self.last_state_time += dt;
            }

            if self.last_state_time > 3.0 && !self.host_lost {
                self.host_lost = true;
            }
        }

        // Advance interpolation
        self.state_lerp_t = (self.state_lerp_t + dt * 20.0).min(1.0); // 20 = NET_TICK_RATE inverse

        if let Some(ref state) = self.client_state {
            self.score = state.your_score as f32;
            self.game_over = state.game_over;

            let my_id = self.my_player_id.unwrap_or(255);
            let our_cells: Vec<&CellState> = state.cells.iter().filter(|c| c.owner_id == my_id).collect();
            if !our_cells.is_empty() {
                let total: f32 = our_cells.iter().map(|c| c.mass).sum();
                let com: Vec2 = our_cells.iter().map(|c| vec2(c.pos[0], c.pos[1]) * c.mass).sum::<Vec2>() / total;
                self.camera_pos = self.camera_pos.lerp(com, 5.0 * dt);
                let avg_radius = mass_to_radius(total);
                let target_zoom = 1.0 / (avg_radius / BASE_RADIUS).sqrt();
                self.camera_zoom += (target_zoom - self.camera_zoom) * 3.0 * dt;
            }
        }
    }

    pub fn snapshot_world_state(&self) -> WorldState {
        let mut cells = Vec::new();

        for c in &self.player_cells {
            cells.push(CellState {
                pos: [c.pos.x, c.pos.y],
                mass: c.mass,
                color: [c.color.r, c.color.g, c.color.b, c.color.a],
                name: c.name.clone(),
                owner_id: 0,
            });
        }

        for (&pid, np) in &self.net_players {
            for c in &np.cells {
                cells.push(CellState {
                    pos: [c.pos.x, c.pos.y],
                    mass: c.mass,
                    color: [c.color.r, c.color.g, c.color.b, c.color.a],
                    name: c.name.clone(),
                    owner_id: pid,
                });
            }
        }

        for c in &self.ai_blobs {
            cells.push(CellState {
                pos: [c.pos.x, c.pos.y],
                mass: c.mass,
                color: [c.color.r, c.color.g, c.color.b, c.color.a],
                name: c.name.clone(),
                owner_id: 255,
            });
        }

        let food: Vec<FoodState> = self.food.iter().map(|f| FoodState {
            pos: [f.pos.x, f.pos.y],
            color: [f.color.r, f.color.g, f.color.b, f.color.a],
        }).collect();

        let ejected: Vec<EjectedState> = self.ejected.iter().map(|e| EjectedState {
            pos: [e.pos.x, e.pos.y],
            color: [e.color.r, e.color.g, e.color.b, e.color.a],
        }).collect();

        let viruses: Vec<VirusState> = self.viruses.iter().map(|v| VirusState {
            pos: [v.pos.x, v.pos.y],
        }).collect();

        let mut entries: Vec<(&str, f32)> = vec![("You", total_mass(&self.player_cells))];
        for np in self.net_players.values() {
            entries.push((&np.name, total_mass(&np.cells)));
        }
        for ai in &self.ai_blobs {
            entries.push((&ai.name, ai.mass));
        }
        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        entries.truncate(10);
        let leaderboard: Vec<LeaderEntry> = entries.iter().map(|(name, mass)| LeaderEntry {
            name: name.to_string(),
            mass: *mass as u32,
            is_you: *name == "You",
        }).collect();

        WorldState {
            cells,
            food,
            ejected,
            viruses,
            leaderboard,
            your_score: total_mass(&self.player_cells) as u32,
            game_over: self.game_over,
        }
    }

    pub fn promote_to_host(&mut self) -> bool {
        let state = match self.client_state.take() {
            Some(s) => s,
            None => return false,
        };

        self.food = state.food.iter().map(|f| Food {
            pos: vec2(f.pos[0], f.pos[1]),
            color: Color::new(f.color[0], f.color[1], f.color[2], f.color[3]),
        }).collect();

        self.ejected = state.ejected.iter().map(|e| EjectedMass {
            pos: vec2(e.pos[0], e.pos[1]),
            velocity: Vec2::ZERO,
            color: Color::new(e.color[0], e.color[1], e.color[2], e.color[3]),
        }).collect();

        self.viruses = state.viruses.iter().map(|v| Virus {
            pos: vec2(v.pos[0], v.pos[1]),
            fed_count: 0,
            velocity: Vec2::ZERO,
        }).collect();

        self.player_cells.clear();
        self.ai_blobs.clear();
        self.ai_brains.clear();

        for c in &state.cells {
            let cell = Cell::new(
                vec2(c.pos[0], c.pos[1]),
                c.mass,
                Color::new(c.color[0], c.color[1], c.color[2], c.color[3]),
                c.name.clone(),
            );
            self.ai_blobs.push(cell);
            self.ai_brains.push(AiBrain::new());
        }

        if self.player_cells.is_empty() {
            let spawn = vec2(
                rand::gen_range(100.0, WORLD_SIZE - 100.0),
                rand::gen_range(100.0, WORLD_SIZE - 100.0),
            );
            self.player_cells = vec![Cell::new(spawn, 10.0, self.player_color, "You".to_string())];
        }

        self.client_net = None;
        self.host_lost = false;
        self.game_over = false;
        self.net_players.clear();

        let host_net = crate::net::start_host();
        self.host_net = Some(host_net);

        true
    }
}
