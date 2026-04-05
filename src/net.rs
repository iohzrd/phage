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
            eprintln!("[host] endpoint online, addr: {:?}", addr);
            let ticket_str = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, serde_json::to_vec(&addr).expect("serialize addr"));
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
                eprintln!("[host] client connected, assigned player_id={}", player_id);

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
                        match bincode::encode_to_vec(&welcome, bincode::config::standard()) {
                            Ok(data) => {
                                eprintln!("[host] sending welcome to player_id={}, {} bytes", player_id, data.len());
                                let _ = send.write(&data);
                                let _ = send.finish();
                            }
                            Err(e) => eprintln!("[host] failed to encode welcome: {}", e),
                        }
                    }

                    // Spawn writer: send state updates to this client
                    let conn_clone = conn.clone();
                    let writer_player_id = player_id;
                    tokio::spawn(async move {
                        let mut count: u64 = 0;
                        while let Some(state) = client_state_rx.recv().await {
                            let msg = HostMessage::State(state);
                            match bincode::encode_to_vec(&msg, bincode::config::standard()) {
                                Ok(data) => {
                                    count += 1;
                                    if count % 100 == 1 {
                                        eprintln!("[host] sending state #{} to player_id={}, {} bytes", count, writer_player_id, data.len());
                                    }
                                    if let Ok(mut send) = conn_clone.open_uni().await {
                                        let _ = send.write(&data);
                                        let _ = send.finish();
                                    } else {
                                        eprintln!("[host] failed to open uni stream for player_id={}", writer_player_id);
                                        break;
                                    }
                                }
                                Err(e) => eprintln!("[host] failed to encode state for player_id={}: {}", writer_player_id, e),
                            }
                        }
                    });

                    // Reader: receive inputs from this client
                    let mut input_count: u64 = 0;
                    loop {
                        match conn.accept_uni().await {
                            Ok(mut recv) => {
                                match recv.read_to_end(64 * 1024).await {
                                    Ok(buf) => {
                                        match bincode::decode_from_slice::<ClientMessage, _>(&buf, bincode::config::standard()) {
                                            Ok((ClientMessage::Input(input), _)) => {
                                                input_count += 1;
                                                if input_count % 100 == 1 {
                                                    eprintln!("[host] recv input #{} from player_id={}, {} bytes", input_count, player_id, buf.len());
                                                }
                                                let _ = input_tx.send((player_id, input));
                                            }
                                            Err(e) => eprintln!("[host] failed to decode input from player_id={}: {}, {} bytes", player_id, e, buf.len()),
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("[host] read error from player_id={}: {}", player_id, e);
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("[host] accept_uni error for player_id={}: {}", player_id, e);
                                break;
                            }
                        }
                    }
                    eprintln!("[host] player_id={} disconnected", player_id);
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
    let addr: EndpointAddr = serde_json::from_slice(&ticket_bytes)
        .expect("Invalid ticket data");
    eprintln!("[client] parsed ticket, connecting to {:?}", addr);

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
            eprintln!("[client] connected to host");

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
                    if let Ok(data) = bincode::encode_to_vec(&msg, bincode::config::standard()) {
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
            let mut msg_count: u64 = 0;
            loop {
                match conn.accept_uni().await {
                    Ok(mut recv) => {
                        match recv.read_to_end(1024 * 1024).await {
                            Ok(buf) => {
                                match bincode::decode_from_slice::<HostMessage, _>(&buf, bincode::config::standard()) {
                                    Ok((msg, _)) => {
                                        msg_count += 1;
                                        if msg_count % 100 == 1 {
                                            eprintln!("[client] recv msg #{}, {} bytes, type: {}", msg_count, buf.len(), match &msg {
                                                HostMessage::Welcome { player_id } => format!("Welcome(player_id={})", player_id),
                                                HostMessage::State(s) => format!("State(cells={}, food={})", s.cells.len(), s.food.len()),
                                            });
                                        }
                                        let _ = state_tx.send(msg);
                                    }
                                    Err(e) => eprintln!("[client] failed to decode msg: {}, {} bytes", e, buf.len()),
                                }
                            }
                            Err(e) => {
                                eprintln!("[client] read error: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[client] accept_uni error: {}", e);
                        break;
                    }
                }
            }
            eprintln!("[client] disconnected from host");
        });
    });

    ClientNet {
        input_tx,
        state_rx,
    }
}
