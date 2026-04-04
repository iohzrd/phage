use std::sync::mpsc;
use std::thread;

use iroh::endpoint::presets;
use iroh::{Endpoint, EndpointAddr};

use crate::protocol::*;

/// Channel-based interface between macroquad main loop and iroh networking thread.

// --- Host side ---

pub struct HostNet {
    /// Receive inputs from connected clients: (player_id, input)
    pub input_rx: mpsc::Receiver<(u8, PlayerInput)>,
    /// Send world state to broadcast to all clients
    pub state_tx: mpsc::Sender<WorldState>,
    /// The ticket/address string to share with clients
    pub ticket: String,
}

pub fn start_host() -> HostNet {
    let (input_tx, input_rx) = mpsc::channel::<(u8, PlayerInput)>();
    let (state_tx, state_rx) = mpsc::channel::<WorldState>();
    let (ticket_tx, ticket_rx) = mpsc::sync_channel::<String>(1);

    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let ep = Endpoint::builder(presets::N0)
                .alpns(vec![GAME_ALPN.to_vec()])
                .bind()
                .await
                .expect("Failed to bind endpoint");

            ep.online().await;
            let addr = ep.addr();
            let ticket_str = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bincode::serde::encode_to_vec(&addr, bincode::config::standard()).expect("serialize addr"));
            ticket_tx.send(ticket_str).ok();

            let input_tx_clone = input_tx.clone();
            let mut next_player_id: u8 = 1;
            let state_rx = std::sync::Arc::new(tokio::sync::Mutex::new(state_rx));

            // Shared list of client senders
            let clients: std::sync::Arc<tokio::sync::Mutex<Vec<tokio::sync::mpsc::Sender<WorldState>>>> =
                std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));

            // Broadcast task: read from state_rx and fan out to all clients
            let clients_clone = clients.clone();
            let state_rx_clone = state_rx.clone();
            tokio::spawn(async move {
                loop {
                    let state = {
                        let rx = state_rx_clone.lock().await;
                        match rx.recv() {
                            Ok(s) => s,
                            Err(_) => break,
                        }
                    };
                    let clients = clients_clone.lock().await;
                    for client_tx in clients.iter() {
                        let _ = client_tx.try_send(state.clone());
                    }
                }
            });

            // Accept loop
            loop {
                let connecting = match ep.accept().await {
                    Some(c) => c,
                    None => break,
                };
                let conn = match connecting.await {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let player_id = next_player_id;
                next_player_id = next_player_id.wrapping_add(1).max(1);

                let input_tx = input_tx_clone.clone();
                let clients = clients.clone();

                tokio::spawn(async move {
                    // Create a channel for this client's state updates
                    let (client_state_tx, mut client_state_rx) =
                        tokio::sync::mpsc::channel::<WorldState>(2);

                    clients.lock().await.push(client_state_tx);

                    // Send welcome
                    if let Ok(mut send) = conn.open_uni().await {
                        let welcome = HostMessage::Welcome { player_id };
                        if let Ok(data) = bincode::serde::encode_to_vec(&welcome, bincode::config::standard()) {
                            let len = (data.len() as u32).to_le_bytes();
                            let _ = send.write(&len);
                            let _ = send.write(&data);
                            let _ = send.finish();
                        }
                    }

                    // Spawn writer: send state updates to this client
                    let conn_clone = conn.clone();
                    tokio::spawn(async move {
                        while let Some(state) = client_state_rx.recv().await {
                            let msg = HostMessage::State(state);
                            if let Ok(data) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
                                if let Ok(mut send) = conn_clone.open_uni().await {
                                    let len = (data.len() as u32).to_le_bytes();
                                    let _ = send.write(&len);
                                    let _ = send.write(&data);
                                    let _ = send.finish();
                                } else {
                                    break;
                                }
                            }
                        }
                    });

                    // Reader: receive inputs from this client
                    loop {
                        match conn.accept_uni().await {
                            Ok(mut recv) => {
                                match recv.read_to_end(64 * 1024).await {
                                    Ok(buf) => {
                                        if let Ok((ClientMessage::Input(input), _)) = bincode::serde::decode_from_slice(&buf, bincode::config::standard()) {
                                            let _ = input_tx.send((player_id, input));
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        });
    });

    let ticket = ticket_rx.recv().expect("Failed to get host ticket");

    HostNet {
        input_rx,
        state_tx,
        ticket,
    }
}

// --- Client side ---

pub struct ClientNet {
    /// Send input to host
    pub input_tx: mpsc::Sender<PlayerInput>,
    /// Receive world state from host
    pub state_rx: mpsc::Receiver<HostMessage>,
}

pub fn start_client(ticket_str: &str) -> ClientNet {
    let (input_tx, input_rx) = mpsc::channel::<PlayerInput>();
    let (state_tx, state_rx) = mpsc::channel::<HostMessage>();

    let ticket_bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, ticket_str)
        .expect("Invalid ticket base64");
    let (addr, _): (EndpointAddr, _) = bincode::serde::decode_from_slice(&ticket_bytes, bincode::config::standard())
        .expect("Invalid ticket data");

    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let ep = Endpoint::builder(presets::N0)
                .alpns(vec![GAME_ALPN.to_vec()])
                .bind()
                .await
                .expect("Failed to bind endpoint");

            ep.online().await;

            let conn = ep.connect(addr, GAME_ALPN)
                .await
                .expect("Failed to connect to host");

            // Spawn writer: send inputs to host
            let conn_clone = conn.clone();
            let input_rx = std::sync::Arc::new(std::sync::Mutex::new(input_rx));
            let input_rx_clone = input_rx.clone();
            tokio::spawn(async move {
                loop {
                    let input = {
                        let rx = input_rx_clone.lock().unwrap();
                        match rx.recv() {
                            Ok(i) => i,
                            Err(_) => break,
                        }
                    };
                    let msg = ClientMessage::Input(input);
                    if let Ok(data) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
                        if let Ok(mut send) = conn_clone.open_uni().await {
                            let _ = send.write(&data);
                            let _ = send.finish();
                        } else {
                            break;
                        }
                    }
                }
            });

            // Reader: receive state from host
            loop {
                match conn.accept_uni().await {
                    Ok(mut recv) => {
                        match recv.read_to_end(1024 * 1024).await {
                            Ok(buf) => {
                                if let Ok((msg, _)) = bincode::serde::decode_from_slice::<HostMessage, _>(&buf, bincode::config::standard()) {
                                    let _ = state_tx.send(msg);
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    });

    ClientNet {
        input_tx,
        state_rx,
    }
}
