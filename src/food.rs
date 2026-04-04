use macroquad::prelude::*;

use crate::constants::*;

pub struct Food {
    pub pos: Vec2,
    pub color: Color,
}

impl Food {
    pub fn random() -> Self {
        Food {
            pos: vec2(
                rand::gen_range(0.0, WORLD_SIZE),
                rand::gen_range(0.0, WORLD_SIZE),
            ),
            color: Color::new(
                rand::gen_range(0.3, 1.0),
                rand::gen_range(0.3, 1.0),
                rand::gen_range(0.3, 1.0),
                1.0,
            ),
        }
    }

    pub fn draw(&self) {
        draw_rectangle(
            self.pos.x - FOOD_RADIUS,
            self.pos.y - FOOD_RADIUS,
            FOOD_RADIUS * 2.0,
            FOOD_RADIUS * 2.0,
            self.color,
        );
    }
}

/// Ejected mass blob (from W key)
pub struct EjectedMass {
    pub pos: Vec2,
    pub velocity: Vec2,
    pub color: Color,
}

impl EjectedMass {
    pub fn new(pos: Vec2, dir: Vec2, color: Color) -> Self {
        EjectedMass {
            pos,
            velocity: dir * EJECT_SPEED,
            color,
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.pos += self.velocity * dt;
        self.velocity *= (0.01_f32).powf(dt);
        // Clamp to world
        self.pos = self.pos.clamp(
            vec2(FOOD_RADIUS, FOOD_RADIUS),
            vec2(WORLD_SIZE - FOOD_RADIUS, WORLD_SIZE - FOOD_RADIUS),
        );
    }

    pub fn draw(&self) {
        draw_circle(self.pos.x, self.pos.y, 8.0, self.color);
    }
}
