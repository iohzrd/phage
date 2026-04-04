use std::f32::consts::TAU;

use macroquad::prelude::*;

use crate::constants::*;

const WOBBLE_POINTS: usize = 24;
const WOBBLE_DAMPING: f32 = 0.7;
const WOBBLE_NOISE: f32 = 1.0;
const WOBBLE_CLAMP: f32 = 10.0;
const GROWTH_LERP_SPEED: f32 = 8.0; // ~120ms at 60fps

pub struct Cell {
    pub pos: Vec2,
    pub mass: f32,
    pub color: Color,
    pub name: String,
    pub velocity: Vec2,
    pub merge_timer: f32,
    // Animation state
    pub display_radius: f32,       // smoothly lerps toward actual radius
    pub wobble_acc: [f32; WOBBLE_POINTS],  // radial displacement per point
    pub wobble_phase: f32,         // rolling angle offset (unique per cell)
    pub id: u32,                   // unique id for wobble phase
}

static mut NEXT_CELL_ID: u32 = 0;

fn next_id() -> u32 {
    unsafe {
        let id = NEXT_CELL_ID;
        NEXT_CELL_ID = NEXT_CELL_ID.wrapping_add(1);
        id
    }
}

impl Cell {
    pub fn new(pos: Vec2, mass: f32, color: Color, name: String) -> Self {
        let id = next_id();
        Cell {
            pos,
            mass,
            color,
            name,
            velocity: Vec2::ZERO,
            merge_timer: 0.0,
            display_radius: mass_to_radius(mass),
            wobble_acc: [0.0; WOBBLE_POINTS],
            wobble_phase: (id as f32) * 0.7, // unique starting phase
            id,
        }
    }

    pub fn radius(&self) -> f32 {
        mass_to_radius(self.mass)
    }

    pub fn speed(&self) -> f32 {
        speed_for_mass(self.mass)
    }

    pub fn can_eat_mass(&self, other_mass: f32) -> bool {
        self.mass > other_mass * EAT_RATIO
    }

    /// Update wobble and smooth growth each frame
    pub fn update_animation(&mut self, dt: f32) {
        // Smooth radius lerp
        let target = self.radius();
        self.display_radius += (target - self.display_radius) * GROWTH_LERP_SPEED * dt;

        // Rolling phase
        self.wobble_phase += dt * 0.5;

        // Update wobble points
        for i in 0..WOBBLE_POINTS {
            // Add random noise
            self.wobble_acc[i] += (rand::gen_range(-1.0_f32, 1.0)) * WOBBLE_NOISE;
            // Damp toward zero
            self.wobble_acc[i] *= WOBBLE_DAMPING;
            // Clamp
            self.wobble_acc[i] = self.wobble_acc[i].clamp(-WOBBLE_CLAMP, WOBBLE_CLAMP);
        }
        // Smooth with neighbors
        let mut smoothed = [0.0_f32; WOBBLE_POINTS];
        for i in 0..WOBBLE_POINTS {
            let prev = self.wobble_acc[(i + WOBBLE_POINTS - 1) % WOBBLE_POINTS];
            let next = self.wobble_acc[(i + 1) % WOBBLE_POINTS];
            smoothed[i] = (prev + next + 8.0 * self.wobble_acc[i]) / 10.0;
        }
        self.wobble_acc = smoothed;
    }

    pub fn draw(&self) {
        self.draw_with_alpha(1.0);
    }

    pub fn draw_with_alpha(&self, alpha: f32) {
        let r = self.display_radius;
        let color = Color::new(self.color.r, self.color.g, self.color.b, self.color.a * alpha);
        let outline = Color::new(0.0, 0.0, 0.0, 0.1 * alpha);

        if r < 15.0 {
            // Small cells: simple circle, no wobble
            draw_circle(self.pos.x, self.pos.y, r, color);
            return;
        }

        // Draw wobbly polygon
        let n = WOBBLE_POINTS;
        for i in 0..n {
            let a1 = (i as f32 / n as f32) * TAU + self.wobble_phase;
            let a2 = ((i + 1) as f32 / n as f32) * TAU + self.wobble_phase;
            let r1 = r + self.wobble_acc[i];
            let r2 = r + self.wobble_acc[(i + 1) % n];

            let p1 = self.pos + vec2(a1.cos(), a1.sin()) * r1;
            let p2 = self.pos + vec2(a2.cos(), a2.sin()) * r2;

            // Fill triangle from center to edge
            draw_triangle(self.pos, p1, p2, color);
        }

        // Outline
        for i in 0..n {
            let a1 = (i as f32 / n as f32) * TAU + self.wobble_phase;
            let a2 = ((i + 1) as f32 / n as f32) * TAU + self.wobble_phase;
            let r1 = r + self.wobble_acc[i];
            let r2 = r + self.wobble_acc[(i + 1) % n];

            let p1 = self.pos + vec2(a1.cos(), a1.sin()) * r1;
            let p2 = self.pos + vec2(a2.cos(), a2.sin()) * r2;
            draw_line(p1.x, p1.y, p2.x, p2.y, 2.0, outline);
        }

        // Name
        let font_size = (r * 0.6).max(14.0).min(40.0) as u16;
        let text_dims = measure_text(&self.name, None, font_size, 1.0);
        draw_text(
            &self.name,
            self.pos.x - text_dims.width / 2.0,
            self.pos.y + text_dims.height / 4.0,
            font_size as f32,
            Color::new(1.0, 1.0, 1.0, alpha),
        );
    }

    pub fn update_velocity(&mut self, dt: f32) {
        if self.velocity.length() > 1.0 {
            self.pos += self.velocity * dt;
            self.velocity *= (0.02_f32).powf(dt);
        } else {
            self.velocity = Vec2::ZERO;
        }
    }

    pub fn clamp_to_world(&mut self) {
        let r = self.radius();
        self.pos = self.pos.clamp(vec2(r, r), vec2(WORLD_SIZE - r, WORLD_SIZE - r));
    }

    pub fn apply_decay(&mut self, dt: f32) {
        if self.mass > DECAY_MIN_MASS {
            self.mass -= self.mass * DECAY_RATE * dt;
        }
    }
}

/// A cell that was just eaten — fades out over 120ms
pub struct DyingCell {
    pub pos: Vec2,
    pub radius: f32,
    pub color: Color,
    pub timer: f32,     // counts down from 0.12
}

impl DyingCell {
    pub fn new(pos: Vec2, radius: f32, color: Color) -> Self {
        DyingCell { pos, radius, color, timer: 0.12 }
    }

    /// Returns false when fully faded
    pub fn update(&mut self, dt: f32) -> bool {
        self.timer -= dt;
        self.timer > 0.0
    }

    pub fn draw(&self) {
        let alpha = (self.timer / 0.12).max(0.0);
        let color = Color::new(self.color.r, self.color.g, self.color.b, self.color.a * alpha);
        draw_circle(self.pos.x, self.pos.y, self.radius, color);
    }
}

/// Total mass of a group of cells
pub fn total_mass(cells: &[Cell]) -> f32 {
    cells.iter().map(|c| c.mass).sum()
}

/// Center of mass of a group of cells
pub fn center_of_mass(cells: &[Cell]) -> Vec2 {
    if cells.is_empty() {
        return Vec2::ZERO;
    }
    let tm = total_mass(cells);
    if tm <= 0.0 {
        return cells[0].pos;
    }
    cells.iter().map(|c| c.pos * c.mass).sum::<Vec2>() / tm
}

/// Push overlapping same-owner cells apart, but only when they can't merge yet
pub fn push_apart(cells: &mut [Cell], dt: f32) {
    for i in 0..cells.len() {
        for j in (i + 1)..cells.len() {
            if cells[i].merge_timer <= 0.0 && cells[j].merge_timer <= 0.0 {
                continue;
            }
            let diff = cells[j].pos - cells[i].pos;
            let dist = diff.length();
            let min_dist = cells[i].radius() + cells[j].radius();
            if dist < min_dist && dist > 0.1 {
                let overlap = min_dist - dist;
                let push = diff.normalize() * overlap * 2.0 * dt;
                cells[i].pos -= push;
                cells[j].pos += push;
            }
        }
    }
}

/// Merge cells whose merge timers have expired
pub fn merge_cells(cells: &mut Vec<Cell>) {
    let mut i = 0;
    while i < cells.len() {
        if cells[i].merge_timer > 0.0 {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < cells.len() {
            if cells[j].merge_timer > 0.0 {
                j += 1;
                continue;
            }
            let dist = cells[i].pos.distance(cells[j].pos);
            let overlap_needed = cells[i].radius().max(cells[j].radius());
            if dist < overlap_needed {
                let total = cells[i].mass + cells[j].mass;
                cells[i].pos =
                    (cells[i].pos * cells[i].mass + cells[j].pos * cells[j].mass) / total;
                cells[i].mass = total;
                cells.remove(j);
            } else {
                j += 1;
            }
        }
        i += 1;
    }
}

pub fn random_color() -> Color {
    Color::new(
        rand::gen_range(0.2, 0.9),
        rand::gen_range(0.2, 0.9),
        rand::gen_range(0.2, 0.9),
        1.0,
    )
}

pub fn random_bot_name() -> String {
    const NAMES: &[&str] = &[
        "Alpha", "Nebula", "Chomper", "Pixel", "Blobby", "Muncher", "Orbiter", "Cosmic",
        "Glider", "Phantom", "Nova", "Drifter", "Pulse", "Ripple", "Surge", "Ember", "Frosty",
        "Vortex", "Nimbus", "Spark",
    ];
    format!("[BOT] {}", NAMES[rand::gen_range(0, NAMES.len())])
}
