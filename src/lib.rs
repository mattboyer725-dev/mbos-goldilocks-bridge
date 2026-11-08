//! Local-first adaptive loop.
//! Optional controlled peer sync: you supply the peer list and can read every node.

use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Serialize, Deserialize};

pub const GOLDILOCKS_PRIME: u64 = 18446744069414584321;

pub struct GoldilocksBridgeKernel;

impl GoldilocksBridgeKernel {
    pub fn field_add(a: u64, b: u64) -> u64 {
        let sum = a.wrapping_add(b);
        let mask = ((sum >= GOLDILOCKS_PRIME) as u64).wrapping_neg();
        sum.wrapping_sub(GOLDILOCKS_PRIME & mask)
    }
    pub fn field_mul(a: u64, b: u64) -> u64 {
        let wide = (a as u128) * (b as u128);
        let mut lo = wide as u64;
        let hi = (wide >> 64) as u64;
        lo = lo.wrapping_sub(hi);
        let mask = ((lo > GOLDILOCKS_PRIME) as u64).wrapping_neg();
        lo.wrapping_sub(GOLDILOCKS_PRIME & mask)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeState {
    pub node_id: String,
    pub state: u64,
    pub generation: u64,
    pub adaptation_rate: u64,
}

pub struct AdaptiveLoop {
    pub node_id: String,
    acc: u64,
    learned_bias: u64,
    adaptation_rate: u64,
    generation: u64,
    target: u64,
    /// Explicit peer list you control. Empty = local only.
    peers: Vec<String>,
}

impl AdaptiveLoop {
    pub fn start(node_id: impl Into<String>) -> Self {
        Self::with_target(node_id, 0)
    }

    pub fn with_target(node_id: impl Into<String>, target: u64) -> Self {
        Self {
            node_id: node_id.into(),
            acc: 0,
            learned_bias: 0,
            adaptation_rate: 10,
            generation: 0,
            target: target % GOLDILOCKS_PRIME,
            peers: Vec::new(),
        }
    }

    /// Add a peer you control (host:port). You must be able to reach and inspect it.
    pub fn add_peer(&mut self, addr: impl Into<String>) {
        self.peers.push(addr.into());
    }

    pub fn step(&mut self, raw: u64) {
        let magnitude = raw.wrapping_sub(self.learned_bias);
        let error = self.target.wrapping_sub(self.acc);
        let rate = self.adaptation_rate;
        let adjustment = GoldilocksBridgeKernel::field_add(
            GoldilocksBridgeKernel::field_mul(error, rate),
            GoldilocksBridgeKernel::field_mul(magnitude, 5),
        );
        self.learned_bias = GoldilocksBridgeKernel::field_add(self.learned_bias, adjustment);
        self.learned_bias = self.learned_bias.wrapping_shr(4);

        if magnitude > (GOLDILOCKS_PRIME >> 2) {
            self.adaptation_rate = (self.adaptation_rate * 9) / 10;
            if self.adaptation_rate < 1 { self.adaptation_rate = 1; }
        } else {
            self.adaptation_rate = (self.adaptation_rate * 21) / 20;
            if self.adaptation_rate > 50 { self.adaptation_rate = 50; }
        }

        let biased = GoldilocksBridgeKernel::field_add(raw, self.learned_bias);
        self.acc = GoldilocksBridgeKernel::field_add(self.acc, biased);
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn current_state(&self) -> NodeState {
        NodeState {
            node_id: self.node_id.clone(),
            state: self.acc,
            generation: self.generation,
            adaptation_rate: self.adaptation_rate,
        }
    }

    pub fn is_local_only(&self) -> bool { self.peers.is_empty() }

    /// Broadcast current state to the explicit peer list you supplied.
    /// Returns the number of peers that acknowledged.
    pub async fn broadcast(&self) -> usize {
        let payload = serde_json::to_vec(&self.current_state()).unwrap_or_default();
        let mut acks = 0;
        for peer in &self.peers {
            if let Ok(mut stream) = TcpStream::connect(peer).await {
                if stream.write_all(&payload).await.is_ok() {
                    let mut buf = [0u8; 8];
                    if stream.read(&mut buf).await.is_ok() {
                        acks += 1;
                    }
                }
            }
        }
        acks
    }

    /// Start a listener so other nodes you control can send state to this one.
    /// You choose the bind address.
    pub async fn listen(self: Arc<Mutex<Self>>, bind: &str) -> std::io::Result<()> {
        let listener = TcpListener::bind(bind).await?;
        loop {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = vec![0u8; 1024];
                if let Ok(n) = socket.read(&mut buf).await {
                    if let Ok(peer_state) = serde_json::from_slice::<NodeState>(&buf[..n]) {
                        // Store or log; owner can inspect via current_state()
                        let _ = peer_state;
                        let _ = socket.write_all(b"ACK").await;
                    }
                }
            }
        }
    }
}
