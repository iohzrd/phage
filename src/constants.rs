pub const WORLD_SIZE: f32 = 10000.0;
pub const FOOD_COUNT: usize = 1500;
pub const AI_COUNT: usize = 30;
pub const VIRUS_COUNT: usize = 8;
pub const BASE_RADIUS: f32 = 20.0;
pub const BASE_SPEED: f32 = 1000.0;
pub const FOOD_RADIUS: f32 = 5.0;
pub const FOOD_MASS: f32 = 1.0;
pub const GRID_SPACING: f32 = 100.0;
pub const EAT_RATIO: f32 = 1.1; // must be 110% of target's mass to eat
pub const MAX_CELLS: usize = 16;
pub const MERGE_TIME_BASE: f32 = 10.0;
pub const MERGE_TIME_SCALE: f32 = 0.01; // extra seconds per mass unit
pub const SPLIT_LAUNCH_SPEED: f32 = 1200.0;
pub const SPLIT_MIN_MASS: f32 = 35.0;
pub const EJECT_MASS_COST: f32 = 13.0;
pub const EJECT_MASS_PELLET: f32 = 12.0;
pub const EJECT_SPEED: f32 = 800.0;
pub const EJECT_MIN_MASS: f32 = 35.0;
pub const VIRUS_MASS: f32 = 100.0;
pub const VIRUS_RADIUS: f32 = 60.0;
pub const VIRUS_FEEDS_TO_SPLIT: usize = 7;
pub const VIRUS_LAUNCH_SPEED: f32 = 600.0;
pub const DECAY_RATE: f32 = 0.002;
pub const DECAY_MIN_MASS: f32 = 15.0;

pub fn mass_to_radius(mass: f32) -> f32 {
    BASE_RADIUS * mass.sqrt()
}

pub fn speed_for_mass(mass: f32) -> f32 {
    BASE_SPEED / mass.sqrt()
}
