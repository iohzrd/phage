use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;

use iroh::endpoint::presets;
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr};
use iroh_gossip::net::{Gossip, GOSSIP_ALPN};

use crate::discovery;
use crate::protocol::*;

/// Channel-based interface between macroquad main loop and iroh networking thread.

// --- Game protocol handler ---

/// Shared state for the game protocol handler.
/// This is cloned into the Router and handles each incoming game connection.
#[derive(Debug, Clone)]
struct GameProtocol {
    input_tx: mpsc::Sender<(u8, PlayerInput)>,
    clients: Arc<tokio::sync::Mutex<Vec<tokio::sync::mpsc::Sender<WorldState>>>>,
    next_player_id: Arc<AtomicU8>,
    player_count: Arc<AtomicU8>,
}

impl ProtocolHandler for GameProtocol {
    async fn accept(&self, conn: iroh::endpoint::Connection) -> Result<(), AcceptError> {
        let player_id = self.next_player_id.fetch_add(1, Ordering::Relaxed).max(1);
        self.player_count.fetch_add(1, Ordering::Relaxed);
        eprintln!("[host] client connected, assigned player_id={}", player_id);

        let (client_state_tx, mut client_state_rx) =
            tokio::sync::mpsc::channel::<WorldState>(2);
        self.clients.lock().await.push(client_state_tx);

        let input_tx = self.input_tx.clone();

        // Send welcome
        if let Ok(mut send) = conn.open_uni().await {
            let welcome = HostMessage::Welcome { player_id };
            match bincode::encode_to_vec(&welcome, bincode::config::standard()) {
                Ok(data) => {
                    eprintln!("[host] sending welcome to player_id={}, {} bytes", player_id, data.len());
                    let _ = send.write(&data).await;
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
                            let _ = send.write(&data).await;
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

        // Reader: receive inputs from this client (this is the long-running part)
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
        self.player_count.fetch_sub(1, Ordering::Relaxed);
        Ok(())
    }
}

// --- Host side ---

pub struct HostNet {
    /// Receive inputs from connected clients: (player_id, input)
    pub input_rx: mpsc::Receiver<(u8, PlayerInput)>,
    /// Send world state to broadcast to all clients
    pub state_tx: mpsc::Sender<WorldState>,
    /// The ticket/address string to share with clients
    pub ticket: String,
    /// Shared player count for discovery announcements
    pub player_count: Arc<AtomicU8>,
}

pub fn start_host() -> HostNet {
    let (input_tx, input_rx) = mpsc::channel::<(u8, PlayerInput)>();
    let (state_tx, state_rx) = mpsc::channel::<WorldState>();
    let (ticket_tx, ticket_rx) = mpsc::sync_channel::<String>(1);
    let player_count = Arc::new(AtomicU8::new(1)); // host counts as 1
    let player_count_clone = player_count.clone();

    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let ep = Endpoint::builder(presets::N0)
                .alpns(vec![GAME_ALPN.to_vec(), GOSSIP_ALPN.to_vec()])
                .bind()
                .await
                .expect("Failed to bind endpoint");

            ep.online().await;
            let addr = ep.addr();
            eprintln!("[host] endpoint online, addr: {:?}", addr);
            let ticket_str = base64::Engine::encode(
                &base64::engine::general_purpose::URL_SAFE_NO_PAD,
                serde_json::to_vec(&addr).expect("serialize addr"),
            );
            ticket_tx.send(ticket_str.clone()).ok();

            // Shared client list for broadcasting state
            let clients: Arc<tokio::sync::Mutex<Vec<tokio::sync::mpsc::Sender<WorldState>>>> =
                Arc::new(tokio::sync::Mutex::new(Vec::new()));

            // Set up gossip for server discovery
            let gossip = Gossip::builder().spawn(ep.clone());

            // Set up game protocol handler
            let game_protocol = GameProtocol {
                input_tx,
                clients: clients.clone(),
                next_player_id: Arc::new(AtomicU8::new(1)),
                player_count: player_count_clone.clone(),
            };

            // Router handles both gossip and game connections
            let router = Router::builder(ep.clone())
                .accept(GOSSIP_ALPN, gossip.clone())
                .accept(GAME_ALPN, game_protocol)
                .spawn();

            // Start broadcasting server announcements
            discovery::start_announcer(gossip, ticket_str, player_count_clone);

            // Broadcast task: read from state_rx and fan out to all clients
            let state_rx = Arc::new(tokio::sync::Mutex::new(state_rx));
            let state_rx_clone = state_rx.clone();
            let clients_clone = clients.clone();
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

            // Keep the runtime alive until router shuts down
            router.shutdown().await.ok();
        });
    });

    let ticket = ticket_rx.recv().expect("Failed to get host ticket");

    HostNet {
        input_rx,
        state_tx,
        ticket,
        player_count,
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

    let ticket_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, ticket_str)
            .expect("Invalid ticket base64");
    let addr: EndpointAddr =
        serde_json::from_slice(&ticket_bytes).expect("Invalid ticket data");
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
            let input_rx = Arc::new(std::sync::Mutex::new(input_rx));
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
                            let _ = send.write(&data).await;
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
                                match bincode::decode_from_slice::<HostMessage, _>(
                                    &buf,
                                    bincode::config::standard(),
                                ) {
                                    Ok((msg, _)) => {
                                        msg_count += 1;
                                        if msg_count % 100 == 1 {
                                            eprintln!(
                                                "[client] recv msg #{}, {} bytes, type: {}",
                                                msg_count,
                                                buf.len(),
                                                match &msg {
                                                    HostMessage::Welcome { player_id } =>
                                                        format!("Welcome(player_id={})", player_id),
                                                    HostMessage::State(s) => format!(
                                                        "State(cells={}, food={})",
                                                        s.cells.len(),
                                                        s.food.len()
                                                    ),
                                                }
                                            );
                                        }
                                        let _ = state_tx.send(msg);
                                    }
                                    Err(e) => eprintln!(
                                        "[client] failed to decode msg: {}, {} bytes",
                                        e,
                                        buf.len()
                                    ),
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

    ClientNet { input_tx, state_rx }
}
