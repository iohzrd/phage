mod ai;
mod blob;
mod constants;
mod food;
mod game;
mod host;
mod net;
mod protocol;
mod render;
mod simulation;
mod virus;

use game::Game;
use macroquad::prelude::*;

#[derive(PartialEq)]
enum Mode {
    Menu,
    Solo,
    Host,
    Client,
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Phage".to_string(),
        platform: miniquad::conf::Platform {
            swap_interval: Some(0),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut mode = Mode::Menu;
    let mut game: Option<Game> = None;
    let mut ticket_input = String::new();
    let mut status_msg = String::new();

    loop {
        match mode {
            Mode::Menu => {
                clear_background(Color::new(0.05, 0.05, 0.05, 1.0));
                let sw = screen_width();
                let sh = screen_height();

                let title = "Phage";
                let dims = measure_text(title, None, 60, 1.0);
                draw_text(title, sw / 2.0 - dims.width / 2.0, sh * 0.2, 60.0, WHITE);

                // Solo button
                let btn_w = 200.0;
                let btn_h = 50.0;
                let solo_x = sw / 2.0 - btn_w / 2.0;
                let solo_y = sh * 0.35;
                draw_rectangle(solo_x, solo_y, btn_w, btn_h, Color::new(0.2, 0.6, 1.0, 1.0));
                let t = "Solo Play";
                let td = measure_text(t, None, 24, 1.0);
                draw_text(t, solo_x + btn_w / 2.0 - td.width / 2.0, solo_y + 32.0, 24.0, WHITE);

                // Host button
                let host_y = sh * 0.48;
                draw_rectangle(solo_x, host_y, btn_w, btn_h, Color::new(0.2, 0.8, 0.3, 1.0));
                let t = "Host Game";
                let td = measure_text(t, None, 24, 1.0);
                draw_text(t, solo_x + btn_w / 2.0 - td.width / 2.0, host_y + 32.0, 24.0, WHITE);

                // Join section
                let join_y = sh * 0.61;
                draw_rectangle(solo_x, join_y, btn_w, btn_h, Color::new(0.8, 0.5, 0.2, 1.0));
                let t = "Join Game";
                let td = measure_text(t, None, 24, 1.0);
                draw_text(t, solo_x + btn_w / 2.0 - td.width / 2.0, join_y + 32.0, 24.0, WHITE);

                // Ticket input field
                let input_y = join_y + btn_h + 10.0;
                draw_rectangle(solo_x - 50.0, input_y, btn_w + 100.0, 30.0, Color::new(0.15, 0.15, 0.15, 1.0));
                draw_rectangle_lines(solo_x - 50.0, input_y, btn_w + 100.0, 30.0, 1.0, GRAY);
                let display = if ticket_input.is_empty() { "Paste ticket here..." } else { &ticket_input };
                draw_text(display, solo_x - 45.0, input_y + 21.0, 16.0, LIGHTGRAY);

                // Status message
                if !status_msg.is_empty() {
                    let sd = measure_text(&status_msg, None, 20, 1.0);
                    draw_text(&status_msg, sw / 2.0 - sd.width / 2.0, sh * 0.85, 20.0, YELLOW);
                }

                // Handle Ctrl+V paste
                if is_key_down(KeyCode::LeftControl) && is_key_pressed(KeyCode::V) {
                    if let Ok(mut clip) = arboard::Clipboard::new() {
                        if let Ok(text) = clip.get_text() {
                            ticket_input = text.trim().to_string();
                        }
                    }
                    // Drain any char events so they don't double-append
                    while get_char_pressed().is_some() {}
                } else if let Some(c) = get_char_pressed() {
                    if c == '\u{8}' || c == '\u{7f}' {
                        ticket_input.pop();
                    } else if c == '\r' || c == '\n' {
                        if !ticket_input.is_empty() {
                            status_msg = "Connecting...".to_string();
                        }
                    } else if !c.is_control() {
                        ticket_input.push(c);
                    }
                }

                // Click detection
                if is_mouse_button_pressed(MouseButton::Left) {
                    let (mx, my) = mouse_position();
                    if mx >= solo_x && mx <= solo_x + btn_w && my >= solo_y && my <= solo_y + btn_h {
                        game = Some(Game::new_solo());
                        mode = Mode::Solo;
                    } else if mx >= solo_x && mx <= solo_x + btn_w && my >= host_y && my <= host_y + btn_h {
                        status_msg = "Starting host...".to_string();
                        let host_net = net::start_host();
                        // Auto-copy ticket to clipboard
                        if let Ok(mut clip) = arboard::Clipboard::new() {
                            let _ = clip.set_text(&host_net.ticket);
                            status_msg = "Ticket copied to clipboard!".to_string();
                        }
                        game = Some(Game::new_host(host_net));
                        mode = Mode::Host;
                    } else if mx >= solo_x && mx <= solo_x + btn_w && my >= join_y && my <= join_y + btn_h {
                        if !ticket_input.is_empty() {
                            status_msg = "Connecting...".to_string();
                            let client_net = net::start_client(&ticket_input);
                            game = Some(Game::new_client(client_net));
                            mode = Mode::Client;
                        } else {
                            status_msg = "Enter a ticket first!".to_string();
                        }
                    }
                }
            }
            Mode::Solo | Mode::Host => {
                if let Some(ref mut g) = game {
                    g.update();
                    g.draw();
                }
            }
            Mode::Client => {
                if let Some(ref mut g) = game {
                    g.update_client();
                    g.draw_client();
                    // Host migration: if host disconnected, promote to host
                    if g.host_lost {
                        if g.promote_to_host() {
                            mode = Mode::Host;
                        }
                    }
                }
            }
        }

        next_frame().await;
    }
}
