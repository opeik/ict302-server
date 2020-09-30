use serde::{Deserialize, Serialize};

use tokio::net::TcpStream;
use tokio::sync::RwLock;

use nalgebra_glm::Vec3;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub type PlayerId = u32;
pub type EntityId = u32;

#[derive(Clone)]
pub struct State {
    pub entities: Arc<RwLock<HashMap<String, Entity>>>,
    pub connections: Arc<RwLock<HashMap<PlayerId, Connection>>>,
    pub player_count: Arc<RwLock<PlayerId>>,
}

#[derive(Debug)]
pub struct Connection {
    pub stream: TcpStream,
    pub addr: SocketAddr,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Entity {
    pub position: Vec3,
    pub angles: Vec3,
    pub velocity: Vec3,
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
    pub fn new(stream: TcpStream, addr: SocketAddr) -> Self {
        Connection { stream, addr }
    }
}
