use serde::{Deserialize, Serialize};

use nalgebra_glm::Vec3;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_serde::{formats::Json, Framed};
use tokio_util::codec::{Framed as CodecFramed, LengthDelimitedCodec};

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub type PlayerId = u32;

#[derive(Clone)]
pub struct State {
    pub entities: Arc<RwLock<HashMap<String, Entity>>>,
    pub connections: Arc<RwLock<HashMap<PlayerId, RwLock<Connection>>>>,
    pub player_count: Arc<RwLock<PlayerId>>,
}
pub struct Connection {
    pub framed: MessageFramed,
    pub addr: SocketAddr,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub struct Entity {
    pub position: Vec3,
    pub angles: Vec3,
    pub velocity: Vec3,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum ClientMessage {
    Hello,
    Acknowledged,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    Hello,
    Update(HashMap<String, Entity>),
}

pub type MessageFramed = Framed<
    CodecFramed<TcpStream, LengthDelimitedCodec>,
    ClientMessage,
    ServerMessage,
    Json<ClientMessage, ServerMessage>,
>;

#[derive(Debug)]
pub enum Error {
    PlayerNotFound { player_id: PlayerId },
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::PlayerNotFound { player_id } => {
                write!(f, "player with id {} not found", player_id)
            }
        }
    }
}

impl State {
    pub fn new() -> Self {
        State {
            entities: Arc::new(RwLock::new(HashMap::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
            player_count: Arc::new(RwLock::new(0)),
        }
    }
}

impl Entity {
    pub fn new(position: Vec3, angles: Vec3, velocity: Vec3) -> Self {
        Entity {
            position,
            angles,
            velocity,
        }
    }
}

impl Connection {
    pub fn new(framed: MessageFramed, addr: SocketAddr) -> Self {
        Connection { framed, addr }
    }
}
