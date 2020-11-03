use serde::{Deserialize, Serialize};

use nalgebra_glm::Vec3;
use tokio::{net::TcpStream, sync::RwLock};
use tokio_serde::{formats::Json, Framed};
use tokio_util::codec::{Framed as CodecFramed, LengthDelimitedCodec};

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::net::SocketAddr;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, Box<dyn StdError + Send + Sync>>;
pub type PlayerId = u32;

fn is_true(v: &bool) -> bool {
    *v
}

#[derive(Clone)]
pub struct State {
    pub entities: Arc<RwLock<HashMap<String, Entity>>>,
    pub history: Arc<RwLock<HashMap<String, Vec<Entity>>>>,
    pub players: Arc<RwLock<HashMap<PlayerId, Player>>>,
    pub connections: Arc<RwLock<HashMap<PlayerId, RwLock<Connection>>>>,
    pub next_player_id: Arc<RwLock<PlayerId>>,
    pub current_time: Arc<RwLock<DateTime<Utc>>>,
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
    #[serde(skip_serializing)]
    pub date_time: DateTime<Utc>,
    #[serde(skip_serializing_if = "is_true")]
    pub is_interpolated: bool,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub struct Player {
    pub position: Vec3,
    pub angles: Vec3,
}
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub struct ClientUpdateData {
    pub position: Vec3,
    pub angles: Vec3,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerUpdateData {
    pub player_id: PlayerId,
    pub entities: HashMap<String, Entity>,
    pub players: HashMap<PlayerId, Player>,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum ClientMessage {
    Disconnect,
    Update(ClientUpdateData),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    Update(ServerUpdateData),
}

pub type MessageFramed = Framed<
    CodecFramed<TcpStream, LengthDelimitedCodec>,
    ClientMessage,
    ServerMessage,
    Json<ClientMessage, ServerMessage>,
>;

impl State {
    pub fn new() -> Self {
        Self {
            entities: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(HashMap::new())),
            players: Arc::new(RwLock::new(HashMap::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
            next_player_id: Arc::new(RwLock::new(0)),
            current_time: Arc::new(RwLock::new(
                DateTime::parse_from_rfc3339("2018-01-02T08:00:00+00:00")
                    .unwrap()
                    .with_timezone(&Utc),
            )),
        }
    }
}

impl Entity {
    pub fn new(
        position: Vec3,
        angles: Vec3,
        velocity: Vec3,
        date_time: DateTime<Utc>,
        is_interpolated: bool,
    ) -> Self {
        Self {
            position,
            angles,
            velocity,
            date_time,
            is_interpolated,
        }
    }
}

impl Player {
    pub fn new(position: Vec3, angles: Vec3) -> Self {
        Self { position, angles }
    }
}

impl Connection {
    pub fn new(framed: MessageFramed, addr: SocketAddr) -> Self {
        Self { framed, addr }
    }
}
