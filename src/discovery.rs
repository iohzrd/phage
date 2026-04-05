use std::sync::mpsc;
use std::thread;

use bytes::Bytes;
use futures_lite::StreamExt;
use iroh::endpoint::presets;
use iroh::protocol::Router;
use iroh::Endpoint;
use iroh_gossip::api::Event;
use iroh_gossip::net::{Gossip, GOSSIP_ALPN};
use iroh_gossip::proto::TopicId;
use serde::{Deserialize, Serialize};

/// Well-known topic for phage server discovery (32 bytes).
pub const DISCOVERY_TOPIC: TopicId =
    TopicId::from_bytes(*b"phage-server-discovery-topic!v1!");

/// Broadcast by hosts every ~12 seconds.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerAnnouncement {
    pub host_name: String,
    pub player_count: u8,
    pub ticket: String,
}

/// Handle returned to the menu for receiving discovered servers.
pub struct DiscoveryHandle {
    pub servers_rx: mpsc::Receiver<ServerAnnouncement>,
}

/// Start a lightweight gossip node that listens for host announcements.
pub fn start_listener() -> DiscoveryHandle {
    let (servers_tx, servers_rx) = mpsc::channel();

    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let ep = Endpoint::builder(presets::N0)
                .alpns(vec![GOSSIP_ALPN.to_vec()])
                .bind()
                .await
                .expect("Failed to bind discovery endpoint");

            ep.online().await;
            eprintln!("[discovery] listener online, id: {}", ep.id());

            let gossip = Gossip::builder().spawn(ep.clone());
            let _router = Router::builder(ep.clone())
                .accept(GOSSIP_ALPN, gossip.clone())
                .spawn();

            let mut topic = gossip
                .subscribe(DISCOVERY_TOPIC, vec![])
                .await
                .expect("Failed to subscribe to discovery topic");

            eprintln!("[discovery] subscribed to topic, waiting for announcements...");

            while let Some(Ok(event)) = topic.next().await {
                if let Event::Received(msg) = event {
                    if let Ok(announcement) =
                        serde_json::from_slice::<ServerAnnouncement>(&msg.content)
                    {
                        eprintln!(
                            "[discovery] found server: {} ({} players)",
                            announcement.host_name, announcement.player_count
                        );
                        let _ = servers_tx.send(announcement);
                    }
                }
            }
        });
    });

    DiscoveryHandle { servers_rx }
}

/// Start broadcasting host announcements on the given gossip instance.
/// Called from within the host's tokio runtime.
pub fn start_announcer(
    gossip: Gossip,
    ticket: String,
    player_count: std::sync::Arc<std::sync::atomic::AtomicU8>,
) {
    tokio::spawn(async move {
        let topic = match gossip.subscribe(DISCOVERY_TOPIC, vec![]).await {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[discovery] failed to subscribe for announcing: {}", e);
                return;
            }
        };
        let (sender, _receiver) = topic.split();

        eprintln!("[discovery] announcer started, broadcasting every 12s");

        loop {
            let announcement = ServerAnnouncement {
                host_name: "Phage Server".to_string(),
                player_count: player_count.load(std::sync::atomic::Ordering::Relaxed),
                ticket: ticket.clone(),
            };

            if let Ok(data) = serde_json::to_vec(&announcement) {
                if let Err(e) = sender.broadcast(Bytes::from(data)).await {
                    eprintln!("[discovery] broadcast error: {}", e);
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(12)).await;
        }
    });
}
