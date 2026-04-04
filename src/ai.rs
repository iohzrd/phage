use macroquad::prelude::*;

use crate::blob::Cell;
use crate::food::Food;

pub struct AiBrain {
    pub target: Option<Vec2>,
    pub retarget_timer: f32,
}

impl AiBrain {
    pub fn new() -> Self {
        AiBrain {
            target: None,
            retarget_timer: 0.0,
        }
    }
}

pub fn update_ai(
    ai_blobs: &mut [Cell],
    ai_brains: &mut [AiBrain],
    player_cells: &[Cell],
    food: &[Food],
    dt: f32,
) {
    // Find the largest player cell for threat assessment
    let player_largest = player_cells
        .iter()
        .max_by(|a, b| a.mass.partial_cmp(&b.mass).unwrap());

    for i in 0..ai_blobs.len() {
        ai_brains[i].retarget_timer -= dt;

        if ai_brains[i].retarget_timer <= 0.0 || ai_brains[i].target.is_none() {
            ai_brains[i].retarget_timer = rand::gen_range(0.5, 2.0);

            let ai_pos = ai_blobs[i].pos;
            let ai_radius = ai_blobs[i].radius();

            let mut flee_from: Option<Vec2> = None;
            let mut flee_dist = f32::MAX;

            // Check all player cells as threats
            for pc in player_cells {
                let d = ai_pos.distance(pc.pos);
                if d < ai_radius * 5.0 && pc.can_eat_mass(ai_blobs[i].mass) && d < flee_dist {
                    flee_dist = d;
                    flee_from = Some(pc.pos);
                }
            }

            // Check other AI
            for (j, other) in ai_blobs.iter().enumerate() {
                if j == i {
                    continue;
                }
                let d = ai_pos.distance(other.pos);
                if d < ai_radius * 5.0 && other.can_eat_mass(ai_blobs[i].mass) && d < flee_dist {
                    flee_dist = d;
                    flee_from = Some(other.pos);
                }
            }

            if let Some(threat) = flee_from {
                let away = (ai_pos - threat).normalize_or_zero();
                ai_brains[i].target = Some(ai_pos + away * 500.0);
            } else {
                let mut best_target: Option<Vec2> = None;
                let mut best_score = f32::MAX;

                // Hunt smaller AI
                for (j, other) in ai_blobs.iter().enumerate() {
                    if j == i {
                        continue;
                    }
                    if ai_blobs[i].can_eat_mass(other.mass) {
                        let d = ai_pos.distance(other.pos);
                        let score = d - other.mass * 10.0;
                        if score < best_score {
                            best_score = score;
                            best_target = Some(other.pos);
                        }
                    }
                }

                // Hunt smaller player cells
                if let Some(pl) = player_largest {
                    if ai_blobs[i].can_eat_mass(pl.mass) {
                        let d = ai_pos.distance(pl.pos);
                        let score = d - pl.mass * 10.0;
                        if score < best_score {
                            best_target = Some(pl.pos);
                        }
                    }
                }

                // Fallback to nearest food
                if best_target.is_none() {
                    let mut best_food_dist = f32::MAX;
                    for f in food {
                        let d = ai_pos.distance(f.pos);
                        if d < best_food_dist {
                            best_food_dist = d;
                            best_target = Some(f.pos);
                        }
                    }
                }

                ai_brains[i].target = best_target;
            }
        }

        if let Some(target) = ai_brains[i].target {
            let dir = target - ai_blobs[i].pos;
            let dist = dir.length();
            if dist > 5.0 {
                let move_dir = dir.normalize();
                let speed = ai_blobs[i].speed();
                ai_blobs[i].pos += move_dir * speed * dt;
            }
        }

        ai_blobs[i].clamp_to_world();
    }
}
