use macroquad::prelude::*;

use crate::ai::{self, AiBrain};
use crate::blob::{center_of_mass, merge_cells, push_apart, random_color, random_bot_name, total_mass, Cell, DyingCell};
use crate::constants::*;
use crate::food::{EjectedMass, Food};
use crate::game::Game;
use crate::virus::Virus;

impl Game {
    pub fn update(&mut self) {
        if self.game_over {
            if is_mouse_button_pressed(MouseButton::Left) || !touches().is_empty() {
                let host_net = self.host_net.take();
                *self = if let Some(hn) = host_net {
                    Game::new_host(hn)
                } else {
                    Game::new_solo()
                };
            }
            return;
        }

        let dt = get_frame_time().min(0.05);

        // --- Local player input ---
        let target_world = self.screen_to_world(mouse_position().into());
        let target_world = if let Some(touch) = touches().first() {
            self.screen_to_world(vec2(touch.position.x, touch.position.y))
        } else {
            target_world
        };

        // Move each player cell toward cursor
        for cell in &mut self.player_cells {
            cell.update_velocity(dt);
            let dir = target_world - cell.pos;
            let dist = dir.length();
            if dist > 5.0 {
                let move_dir = dir.normalize();
                let speed = cell.speed();
                cell.pos += move_dir * speed * dt;
            }
            cell.clamp_to_world();
            cell.merge_timer = (cell.merge_timer - dt).max(0.0);
            cell.update_animation(dt);
        }

        push_apart(&mut self.player_cells, dt);
        merge_cells(&mut self.player_cells);

        if is_key_pressed(KeyCode::Space) {
            self.split_player(target_world);
        }
        if is_key_pressed(KeyCode::W) {
            self.eject_mass(target_world);
        }

        // --- AI update ---
        ai::update_ai(
            &mut self.ai_blobs,
            &mut self.ai_brains,
            &self.player_cells,
            &self.food,
            dt,
        );

        // --- Update AI animations ---
        for ai in &mut self.ai_blobs {
            ai.update_animation(dt);
        }

        // --- Update dying cells ---
        self.dying_cells.retain_mut(|d| d.update(dt));

        // --- Update ejected mass ---
        for e in &mut self.ejected {
            e.update(dt);
        }

        // --- Update viruses ---
        for v in &mut self.viruses {
            v.update(dt);
        }

        // --- Player cells eat food ---
        for cell in &mut self.player_cells {
            let r = cell.radius();
            let before = self.food.len();
            self.food.retain(|f| cell.pos.distance(f.pos) >= r);
            let eaten = before - self.food.len();
            cell.mass += eaten as f32 * FOOD_MASS;
        }

        // --- Player cells eat ejected mass ---
        for cell in &mut self.player_cells {
            let r = cell.radius();
            let before = self.ejected.len();
            self.ejected.retain(|e| cell.pos.distance(e.pos) >= r);
            let eaten = before - self.ejected.len();
            cell.mass += eaten as f32 * EJECT_MASS_PELLET;
        }

        self.check_player_virus_collisions();

        // --- AI eats food ---
        for ai in &mut self.ai_blobs {
            let r = ai.radius();
            let before = self.food.len();
            self.food.retain(|f| ai.pos.distance(f.pos) >= r);
            let eaten = before - self.food.len();
            ai.mass += eaten as f32 * FOOD_MASS;
        }

        // --- AI eats ejected mass ---
        for ai in &mut self.ai_blobs {
            let r = ai.radius();
            let before = self.ejected.len();
            self.ejected.retain(|e| ai.pos.distance(e.pos) >= r);
            let eaten = before - self.ejected.len();
            ai.mass += eaten as f32 * EJECT_MASS_PELLET;
        }

        self.feed_viruses();

        // --- Player eats AI / AI eats player ---
        let mut eaten_ai = Vec::new();
        for (i, ai) in self.ai_blobs.iter().enumerate() {
            for cell in &mut self.player_cells {
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
        eaten_ai.dedup();
        for i in eaten_ai.into_iter().rev() {
            let ai = &self.ai_blobs[i];
            self.dying_cells.push(DyingCell::new(ai.pos, ai.display_radius, ai.color));
            self.ai_blobs.remove(i);
            self.ai_brains.remove(i);
        }

        // AI eats player cells
        let mut eaten_player_cells = Vec::new();
        for ai in &mut self.ai_blobs {
            for (ci, cell) in self.player_cells.iter().enumerate() {
                let dist = ai.pos.distance(cell.pos);
                if dist < ai.radius() && ai.can_eat_mass(cell.mass) {
                    ai.mass += cell.mass * 0.8;
                    if !eaten_player_cells.contains(&ci) {
                        eaten_player_cells.push(ci);
                    }
                }
            }
        }
        eaten_player_cells.sort_unstable();
        for i in eaten_player_cells.into_iter().rev() {
            self.player_cells.remove(i);
        }
        if self.player_cells.is_empty() {
            if self.host_net.is_some() {
                self.respawn_host_player();
            } else {
                self.game_over = true;
            }
        }

        // --- AI eats AI ---
        let mut ai_eaten = Vec::new();
        let ai_count = self.ai_blobs.len();
        for i in 0..ai_count {
            if ai_eaten.contains(&i) {
                continue;
            }
            for j in (i + 1)..ai_count {
                if ai_eaten.contains(&j) {
                    continue;
                }
                let dist = self.ai_blobs[i].pos.distance(self.ai_blobs[j].pos);
                if dist < self.ai_blobs[i].radius() && self.ai_blobs[i].can_eat_mass(self.ai_blobs[j].mass) {
                    ai_eaten.push(j);
                } else if dist < self.ai_blobs[j].radius() && self.ai_blobs[j].can_eat_mass(self.ai_blobs[i].mass) {
                    ai_eaten.push(i);
                }
            }
        }
        ai_eaten.sort_unstable();
        ai_eaten.dedup();
        for &idx in ai_eaten.iter().rev() {
            let eaten_mass = self.ai_blobs[idx].mass;
            let eaten_pos = self.ai_blobs[idx].pos;
            let mut best_eater = None;
            let mut best_dist = f32::MAX;
            for (i, ai) in self.ai_blobs.iter().enumerate() {
                if i == idx || ai_eaten.contains(&i) {
                    continue;
                }
                let d = ai.pos.distance(eaten_pos);
                if d < best_dist && ai.can_eat_mass(self.ai_blobs[idx].mass) {
                    best_dist = d;
                    best_eater = Some(i);
                }
            }
            if let Some(eater) = best_eater {
                self.ai_blobs[eater].mass += eaten_mass * 0.8;
            }
            self.ai_blobs.remove(idx);
            self.ai_brains.remove(idx);
        }

        // --- Respawns ---
        while self.food.len() < FOOD_COUNT {
            self.food.push(Food::random());
        }
        while self.ai_blobs.len() < AI_COUNT {
            self.ai_blobs.push(Cell::new(
                vec2(
                    rand::gen_range(100.0, WORLD_SIZE - 100.0),
                    rand::gen_range(100.0, WORLD_SIZE - 100.0),
                ),
                rand::gen_range(5.0, 20.0),
                random_color(),
                random_bot_name(),
            ));
            self.ai_brains.push(AiBrain::new());
        }
        while self.viruses.len() < VIRUS_COUNT {
            self.viruses.push(Virus::random());
        }

        // --- Decay ---
        for cell in &mut self.player_cells {
            cell.apply_decay(dt);
        }
        for ai in &mut self.ai_blobs {
            ai.apply_decay(dt);
        }

        // --- Camera ---
        if !self.player_cells.is_empty() {
            let com = center_of_mass(&self.player_cells);
            self.camera_pos = self.camera_pos.lerp(com, 5.0 * dt);
            let tm = total_mass(&self.player_cells);
            let avg_radius = mass_to_radius(tm);
            let target_zoom = 1.0 / (avg_radius / BASE_RADIUS).sqrt();
            self.camera_zoom += (target_zoom - self.camera_zoom) * 3.0 * dt;
            self.score = tm;
        }

        // --- Host networking ---
        self.update_host(dt);
    }

    pub fn split_player(&mut self, target: Vec2) {
        let mut new_cells = Vec::new();
        let current_count = self.player_cells.len();
        for cell in &mut self.player_cells {
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
        self.player_cells.extend(new_cells);
    }

    pub fn eject_mass(&mut self, target: Vec2) {
        for cell in &mut self.player_cells {
            if cell.mass < EJECT_MIN_MASS {
                continue;
            }
            cell.mass -= EJECT_MASS_COST;
            let dir = (target - cell.pos).normalize_or_zero();
            let spawn_pos = cell.pos + dir * (cell.radius() + 10.0);
            self.ejected.push(EjectedMass::new(spawn_pos, dir, cell.color));
        }
    }

    pub fn respawn_host_player(&mut self) {
        let spawn = vec2(
            rand::gen_range(100.0, WORLD_SIZE - 100.0),
            rand::gen_range(100.0, WORLD_SIZE - 100.0),
        );
        self.player_cells = vec![Cell::new(spawn, 10.0, self.player_color, "You".to_string())];
    }

    fn check_player_virus_collisions(&mut self) {
        let mut new_cells = Vec::new();
        let current_count = self.player_cells.len();
        let mut viruses_to_remove = Vec::new();

        for cell in &mut self.player_cells {
            if cell.mass <= VIRUS_MASS {
                continue;
            }
            let mut hit_virus = None;
            for (vi, v) in self.viruses.iter().enumerate() {
                if viruses_to_remove.contains(&vi) {
                    continue;
                }
                if cell.pos.distance(v.pos) < cell.radius() {
                    hit_virus = Some(vi);
                    break;
                }
            }
            if let Some(vi) = hit_virus {
                viruses_to_remove.push(vi);
                let pieces = (MAX_CELLS - current_count - new_cells.len()).min(8).max(1);
                let mass_per = cell.mass / (pieces as f32 + 1.0);
                cell.mass = mass_per;
                let merge_time = MERGE_TIME_BASE + mass_per * MERGE_TIME_SCALE;
                cell.merge_timer = merge_time;
                for k in 0..pieces {
                    let angle = (k as f32 / pieces as f32) * std::f32::consts::TAU
                        + rand::gen_range(-0.3, 0.3);
                    let dir = vec2(angle.cos(), angle.sin());
                    let mut new = Cell::new(cell.pos, mass_per, cell.color, cell.name.clone());
                    new.velocity = dir * SPLIT_LAUNCH_SPEED * rand::gen_range(0.5, 1.0);
                    new.merge_timer = merge_time;
                    new_cells.push(new);
                }
            }
        }

        viruses_to_remove.sort_unstable();
        for i in viruses_to_remove.into_iter().rev() {
            self.viruses.remove(i);
        }
        self.player_cells.extend(new_cells);
    }

    fn feed_viruses(&mut self) {
        let mut new_viruses = Vec::new();
        let mut eaten_ejected = Vec::new();

        for (ei, e) in self.ejected.iter().enumerate() {
            for v in self.viruses.iter_mut() {
                if v.pos.distance(e.pos) < VIRUS_RADIUS {
                    v.fed_count += 1;
                    eaten_ejected.push(ei);
                    if v.fed_count >= VIRUS_FEEDS_TO_SPLIT {
                        v.fed_count = 0;
                        let dir = (e.pos - v.pos).normalize_or_zero();
                        let mut new_v = Virus::random();
                        new_v.pos = v.pos + dir * (VIRUS_RADIUS * 2.0);
                        new_v.velocity = dir * VIRUS_LAUNCH_SPEED;
                        new_viruses.push(new_v);
                    }
                    break;
                }
            }
        }

        eaten_ejected.sort_unstable();
        eaten_ejected.dedup();
        for i in eaten_ejected.into_iter().rev() {
            self.ejected.remove(i);
        }
        self.viruses.extend(new_viruses);
    }
}
