use macroquad::prelude::*;

use crate::blob::{total_mass, Cell};
use crate::constants::*;
use crate::game::Game;
use crate::protocol::*;

impl Game {
    pub fn draw(&mut self) {
        set_default_camera();
        clear_background(Color::new(0.05, 0.05, 0.05, 1.0));

        let sw = screen_width();
        let sh = screen_height();

        let cam = Camera2D {
            target: self.camera_pos,
            zoom: vec2(self.camera_zoom * 2.0 / sw, self.camera_zoom * 2.0 / sh),
            ..Default::default()
        };
        set_camera(&cam);

        let view_w = sw / self.camera_zoom;
        let view_h = sh / self.camera_zoom;
        let vp_left = self.camera_pos.x - view_w / 2.0;
        let vp_right = self.camera_pos.x + view_w / 2.0;
        let vp_top = self.camera_pos.y - view_h / 2.0;
        let vp_bottom = self.camera_pos.y + view_h / 2.0;

        self.draw_grid();
        draw_rectangle_lines(0.0, 0.0, WORLD_SIZE, WORLD_SIZE, 4.0, GRAY);

        // Food (culled)
        for f in &self.food {
            if f.pos.x >= vp_left - FOOD_RADIUS
                && f.pos.x <= vp_right + FOOD_RADIUS
                && f.pos.y >= vp_top - FOOD_RADIUS
                && f.pos.y <= vp_bottom + FOOD_RADIUS
            {
                f.draw();
            }
        }

        // Dying cells (fading out)
        for d in &self.dying_cells {
            d.draw();
        }

        // Ejected mass
        for e in &self.ejected {
            if e.pos.x >= vp_left - 10.0
                && e.pos.x <= vp_right + 10.0
                && e.pos.y >= vp_top - 10.0
                && e.pos.y <= vp_bottom + 10.0
            {
                e.draw();
            }
        }

        // Viruses
        for v in &self.viruses {
            if v.pos.x >= vp_left - VIRUS_RADIUS
                && v.pos.x <= vp_right + VIRUS_RADIUS
                && v.pos.y >= vp_top - VIRUS_RADIUS
                && v.pos.y <= vp_bottom + VIRUS_RADIUS
            {
                v.draw();
            }
        }

        // AI blobs (sorted by size, culled)
        let mut sorted_ai: Vec<&Cell> = self.ai_blobs.iter().collect();
        sorted_ai.sort_by(|a, b| a.mass.partial_cmp(&b.mass).unwrap());
        for ai in &sorted_ai {
            let r = ai.radius();
            if ai.pos.x >= vp_left - r
                && ai.pos.x <= vp_right + r
                && ai.pos.y >= vp_top - r
                && ai.pos.y <= vp_bottom + r
            {
                ai.draw();
            }
        }

        // Net player cells
        for np in self.net_players.values() {
            for cell in &np.cells {
                let r = cell.radius();
                if cell.pos.x >= vp_left - r
                    && cell.pos.x <= vp_right + r
                    && cell.pos.y >= vp_top - r
                    && cell.pos.y <= vp_bottom + r
                {
                    cell.draw();
                }
            }
        }

        // Player cells (sorted by size)
        let mut sorted_player: Vec<&Cell> = self.player_cells.iter().collect();
        sorted_player.sort_by(|a, b| a.mass.partial_cmp(&b.mass).unwrap());
        for cell in &sorted_player {
            cell.draw();
        }

        // HUD
        set_default_camera();
        self.draw_hud(sw, sh);

        // Host: "Copy Ticket" button
        if let Some(ref host_net) = self.host_net {
            let btn_w = 120.0;
            let btn_h = 30.0;
            let btn_x = 10.0;
            let btn_y = sh - btn_h - 10.0;

            if self.copied_timer > 0.0 {
                draw_rectangle(btn_x, btn_y, btn_w, btn_h, Color::new(0.1, 0.5, 0.2, 0.9));
                draw_text("Copied!", btn_x + 20.0, btn_y + 21.0, 18.0, WHITE);
            } else {
                draw_rectangle(btn_x, btn_y, btn_w, btn_h, Color::new(0.2, 0.8, 0.3, 0.9));
                draw_text("Copy Ticket", btn_x + 10.0, btn_y + 21.0, 18.0, WHITE);
            }

            if is_mouse_button_pressed(MouseButton::Left) {
                let (mx, my) = mouse_position();
                if mx >= btn_x && mx <= btn_x + btn_w && my >= btn_y && my <= btn_y + btn_h {
                    if let Ok(mut clip) = arboard::Clipboard::new() {
                        let _ = clip.set_text(&host_net.ticket);
                        self.copied_timer = 2.0;
                    }
                }
            }

            self.copied_timer = (self.copied_timer - get_frame_time()).max(0.0);
        }
    }

    pub fn draw_client(&self) {
        clear_background(Color::new(0.05, 0.05, 0.05, 1.0));

        let sw = screen_width();
        let sh = screen_height();

        let cam = Camera2D {
            target: self.camera_pos,
            zoom: vec2(self.camera_zoom * 2.0 / sw, self.camera_zoom * 2.0 / sh),
            ..Default::default()
        };
        set_camera(&cam);

        self.draw_grid();
        draw_rectangle_lines(0.0, 0.0, WORLD_SIZE, WORLD_SIZE, 4.0, GRAY);

        if let Some(ref state) = self.client_state {
            // Food
            for f in &state.food {
                let color = Color::new(f.color[0], f.color[1], f.color[2], f.color[3]);
                draw_rectangle(
                    f.pos[0] - FOOD_RADIUS,
                    f.pos[1] - FOOD_RADIUS,
                    FOOD_RADIUS * 2.0,
                    FOOD_RADIUS * 2.0,
                    color,
                );
            }

            // Ejected mass
            for e in &state.ejected {
                let color = Color::new(e.color[0], e.color[1], e.color[2], e.color[3]);
                draw_circle(e.pos[0], e.pos[1], 8.0, color);
            }

            // Viruses
            for v in &state.viruses {
                draw_circle(v.pos[0], v.pos[1], VIRUS_RADIUS, Color::new(0.2, 0.8, 0.2, 0.7));
            }

            // Cells (sorted by mass) with interpolation
            let t = self.state_lerp_t;
            let mut sorted: Vec<&CellState> = state.cells.iter().collect();
            sorted.sort_by(|a, b| a.mass.partial_cmp(&b.mass).unwrap());
            for c in &sorted {
                let color = Color::new(c.color[0], c.color[1], c.color[2], c.color[3]);
                let radius = mass_to_radius(c.mass);

                // Interpolate position from previous state
                let pos = if let Some(ref prev) = self.prev_state {
                    if let Some(pc) = prev.cells.iter().find(|pc| pc.cell_id == c.cell_id) {
                        let prev_pos = vec2(pc.pos[0], pc.pos[1]);
                        let curr_pos = vec2(c.pos[0], c.pos[1]);
                        prev_pos.lerp(curr_pos, t)
                    } else {
                        vec2(c.pos[0], c.pos[1])
                    }
                } else {
                    vec2(c.pos[0], c.pos[1])
                };

                draw_circle(pos.x, pos.y, radius, color);
                draw_circle_lines(pos.x, pos.y, radius, 2.0, Color::new(0.0, 0.0, 0.0, 0.3));
                let font_size = (radius * 0.6).max(14.0).min(40.0) as u16;
                let text_dims = measure_text(&c.name, None, font_size, 1.0);
                draw_text(
                    &c.name,
                    pos.x - text_dims.width / 2.0,
                    pos.y + text_dims.height / 4.0,
                    font_size as f32,
                    WHITE,
                );
            }
        }

        // HUD
        set_default_camera();
        self.draw_hud(sw, sh);
    }

    pub fn draw_hud(&self, sw: f32, sh: f32) {
        draw_text(&format!("Score: {}", self.score as u32), 10.0, 30.0, 30.0, LIGHTGRAY);
        draw_text(&format!("FPS: {}", get_fps()), 10.0, 60.0, 24.0, LIGHTGRAY);

        if let Some(ref state) = self.client_state {
            // Rebuild leaderboard with correct "is_you" for this client
            let my_id = self.my_player_id.unwrap_or(255);
            let corrected: Vec<LeaderEntry> = state.leaderboard.iter().map(|e| {
                // Find if this entry corresponds to us by checking owner_ids in cells
                let is_me = state.cells.iter().any(|c| c.owner_id == my_id && c.name == e.name);
                LeaderEntry {
                    name: e.name.clone(),
                    mass: e.mass,
                    is_you: is_me,
                }
            }).collect();
            self.draw_leaderboard_from(&corrected);
        } else {
            self.draw_leaderboard();
        }

        self.draw_minimap();

        if self.game_over {
            draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.6));
            let go_text = "Game Over";
            let dims = measure_text(go_text, None, 60, 1.0);
            draw_text(go_text, sw / 2.0 - dims.width / 2.0, sh / 2.0 - 20.0, 60.0, WHITE);

            let score_text = format!("Score: {}", self.score as u32);
            let dims2 = measure_text(&score_text, None, 30, 1.0);
            draw_text(&score_text, sw / 2.0 - dims2.width / 2.0, sh / 2.0 + 20.0, 30.0, WHITE);

            let tap_text = "Tap to restart";
            let dims3 = measure_text(tap_text, None, 24, 1.0);
            draw_text(tap_text, sw / 2.0 - dims3.width / 2.0, sh / 2.0 + 60.0, 24.0, LIGHTGRAY);
        }
    }

    pub fn screen_to_world(&self, screen_pos: Vec2) -> Vec2 {
        let sw = screen_width();
        let sh = screen_height();
        let view_w = sw / self.camera_zoom;
        let view_h = sh / self.camera_zoom;
        let left = self.camera_pos.x - view_w / 2.0;
        let top = self.camera_pos.y - view_h / 2.0;
        vec2(
            left + screen_pos.x / self.camera_zoom,
            top + screen_pos.y / self.camera_zoom,
        )
    }

    fn draw_grid(&self) {
        let sw = screen_width();
        let sh = screen_height();
        let view_w = sw / self.camera_zoom;
        let view_h = sh / self.camera_zoom;
        let left = (self.camera_pos.x - view_w / 2.0).max(0.0);
        let right = (self.camera_pos.x + view_w / 2.0).min(WORLD_SIZE);
        let top = (self.camera_pos.y - view_h / 2.0).max(0.0);
        let bottom = (self.camera_pos.y + view_h / 2.0).min(WORLD_SIZE);

        let grid_color = Color::new(0.15, 0.15, 0.15, 1.0);
        let start_x = (left / GRID_SPACING).floor() * GRID_SPACING;
        let start_y = (top / GRID_SPACING).floor() * GRID_SPACING;

        let mut x = start_x;
        while x <= right {
            draw_line(x, top, x, bottom, 1.0, grid_color);
            x += GRID_SPACING;
        }
        let mut y = start_y;
        while y <= bottom {
            draw_line(left, y, right, y, 1.0, grid_color);
            y += GRID_SPACING;
        }
    }

    fn draw_leaderboard(&self) {
        let sw = screen_width();
        let x = sw - 160.0;
        let y_start = 10.0;

        draw_rectangle(x - 10.0, y_start, 160.0, 30.0 + 10.0 * 22.0, Color::new(0.0, 0.0, 0.0, 0.3));
        draw_text("Leaderboard", x, y_start + 22.0, 20.0, WHITE);

        let mut entries: Vec<(&str, f32)> = vec![("You", total_mass(&self.player_cells))];
        for np in self.net_players.values() {
            entries.push((&np.name, total_mass(&np.cells)));
        }
        for ai in &self.ai_blobs {
            entries.push((&ai.name, ai.mass));
        }
        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        entries.truncate(10);

        for (i, (name, mass)) in entries.iter().enumerate() {
            let text = format!("{}. {} ({})", i + 1, name, *mass as u32);
            let color = if *name == "You" { YELLOW } else { WHITE };
            draw_text(&text, x, y_start + 44.0 + i as f32 * 22.0, 16.0, color);
        }
    }

    fn draw_leaderboard_from(&self, leaderboard: &[LeaderEntry]) {
        let sw = screen_width();
        let x = sw - 160.0;
        let y_start = 10.0;

        draw_rectangle(x - 10.0, y_start, 160.0, 30.0 + 10.0 * 22.0, Color::new(0.0, 0.0, 0.0, 0.3));
        draw_text("Leaderboard", x, y_start + 22.0, 20.0, WHITE);

        for (i, entry) in leaderboard.iter().enumerate() {
            let text = format!("{}. {} ({})", i + 1, entry.name, entry.mass);
            let color = if entry.is_you { YELLOW } else { WHITE };
            draw_text(&text, x, y_start + 44.0 + i as f32 * 22.0, 16.0, color);
        }
    }

    fn draw_minimap(&self) {
        let sw = screen_width();
        let sh = screen_height();
        let map_size = 150.0;
        let map_x = sw - map_size - 10.0;
        let map_y = sh - map_size - 10.0;
        let scale = map_size / WORLD_SIZE;

        draw_rectangle(map_x, map_y, map_size, map_size, Color::new(0.0, 0.0, 0.0, 0.4));
        draw_rectangle_lines(map_x, map_y, map_size, map_size, 1.0, WHITE);

        if let Some(ref state) = self.client_state {
            let my_id = self.my_player_id.unwrap_or(255);
            for c in state.cells.iter().filter(|c| c.owner_id == my_id) {
                let mx = map_x + c.pos[0] * scale;
                let my = map_y + c.pos[1] * scale;
                draw_circle(mx, my, 3.0, self.player_color);
            }
        } else {
            for cell in &self.player_cells {
                let mx = map_x + cell.pos.x * scale;
                let my = map_y + cell.pos.y * scale;
                let mr = (cell.radius() * scale).max(2.0);
                draw_circle(mx, my, mr, self.player_color);
            }
        }

        let view_w = sw / self.camera_zoom;
        let view_h = sh / self.camera_zoom;
        let vx = map_x + (self.camera_pos.x - view_w / 2.0) * scale;
        let vy = map_y + (self.camera_pos.y - view_h / 2.0) * scale;
        draw_rectangle_lines(vx, vy, view_w * scale, view_h * scale, 1.0, Color::new(1.0, 1.0, 1.0, 0.5));
    }
}
