use serde::{Deserialize, Serialize};

use nalgebra_glm::Vec3;
use tokio::{net::TcpStream, sync::RwLock};
use tokio_serde::{formats::Json, Framed};
use tokio_util::codec::{Framed as CodecFramed, LengthDelimitedCodec};

use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};
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
    pub history: Arc<RwLock<HashMap<String, History>>>,
    pub players: Arc<RwLock<HashMap<PlayerId, Player>>>,
    pub connections: Arc<RwLock<HashMap<PlayerId, RwLock<Connection>>>>,
    pub next_player_id: Arc<RwLock<PlayerId>>,
    pub anomalies: Arc<RwLock<Vec<Anomaly>>>,
    pub fetched_new_data: Arc<RwLock<bool>>,
}

pub struct Connection {
    pub framed: MessageFramed,
    pub addr: SocketAddr,
}

pub struct History {
    pub history: Vec<Entity>,
    pub was_updated: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Entity {
    pub position: Vec3,
    pub angles: Vec3,
    pub velocity: Vec3,
    #[serde(skip_serializing)]
    pub date_time: DateTime<Utc>,
    #[serde(skip_serializing_if = "is_true")]
    pub is_interpolated: bool,
    #[serde(skip)]
    pub prediction_cache: VecDeque<Prediction>,
    #[serde(skip_serializing)]
    pub num_stale_predictions: i32,
    pub prediction_kind: Option<crate::prediction::PredictionKind>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Player {
    pub position: Vec3,
    pub angles: Vec3,
    pub is_authenticated: bool,
    pub did_send_auth_status: bool,
    pub username: String,
}
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub struct ClientUpdateData {
    pub position: Vec3,
    pub angles: Vec3,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClientAuthData {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug)]
pub struct Prediction {
    pub position: Vec3,
    pub error: Vec3,
    pub date_time: DateTime<Utc>,
    pub kind: crate::prediction::PredictionKind,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Anomaly {
    pub entity_id: String,
    pub msg: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerUpdateData {
    pub player_id: PlayerId,
    pub entities: HashMap<String, Entity>,
    pub players: HashMap<PlayerId, Player>,
    pub anomalies: Vec<Anomaly>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerAuthenticateResponseData {
    pub success: bool,
    pub msg: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ClientMessage {
    Disconnect,
    Authenticate(ClientAuthData),
    Update(ClientUpdateData),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ServerMessage {
    Update(ServerUpdateData),
    AuthenticateResponse(ServerAuthenticateResponseData),
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
            anomalies: Arc::new(RwLock::new(Vec::new())),
            fetched_new_data: Arc::new(RwLock::new(false)),
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
        prediction_cache: VecDeque<Prediction>,
    ) -> Self {
        Self {
            position,
            angles,
            velocity,
            date_time,
            is_interpolated,
            prediction_cache,
            num_stale_predictions: 0,
            prediction_kind: None,
        }
    }
}

impl Player {
    pub fn new(position: Vec3, angles: Vec3) -> Self {
        Self {
            position,
            angles,
            is_authenticated: false,
            did_send_auth_status: false,
            username: "".to_owned(),
        }
    }
}

impl Connection {
    pub fn new(framed: MessageFramed, addr: SocketAddr) -> Self {
        Self { framed, addr }
    }
}
