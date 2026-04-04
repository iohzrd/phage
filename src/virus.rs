use macroquad::prelude::*;
use std::f32::consts::TAU;

use crate::constants::*;

pub struct Virus {
    pub pos: Vec2,
    pub fed_count: usize,
    pub velocity: Vec2, // for newly launched viruses
}

impl Virus {
    pub fn random() -> Self {
        Virus {
            pos: vec2(
                rand::gen_range(VIRUS_RADIUS, WORLD_SIZE - VIRUS_RADIUS),
                rand::gen_range(VIRUS_RADIUS, WORLD_SIZE - VIRUS_RADIUS),
            ),
            fed_count: 0,
            velocity: Vec2::ZERO,
        }
    }

    pub fn update(&mut self, dt: f32) {
        if self.velocity.length() > 1.0 {
            self.pos += self.velocity * dt;
            self.velocity *= (0.02_f32).powf(dt);
            self.pos = self.pos.clamp(
                vec2(VIRUS_RADIUS, VIRUS_RADIUS),
                vec2(WORLD_SIZE - VIRUS_RADIUS, WORLD_SIZE - VIRUS_RADIUS),
            );
        } else {
            self.velocity = Vec2::ZERO;
        }
    }

    pub fn draw(&self) {
        let spikes = 20;
        let inner_r = VIRUS_RADIUS * 0.8;
        let outer_r = VIRUS_RADIUS;
        let color = Color::new(0.2, 0.8, 0.2, 0.7);

        // Draw spiky shape as triangles from center
        for i in 0..spikes {
            let angle1 = (i as f32 / spikes as f32) * TAU;
            let angle2 = ((i as f32 + 0.5) / spikes as f32) * TAU;
            let angle3 = ((i as f32 + 1.0) / spikes as f32) * TAU;

            let p1 = self.pos + vec2(angle1.cos(), angle1.sin()) * inner_r;
            let p2 = self.pos + vec2(angle2.cos(), angle2.sin()) * outer_r;
            let p3 = self.pos + vec2(angle3.cos(), angle3.sin()) * inner_r;

            draw_triangle(p1, p2, p3, color);
        }
        // Fill center
        draw_circle(self.pos.x, self.pos.y, inner_r, color);
    }
}
