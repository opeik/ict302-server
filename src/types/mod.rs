use serde::{Deserialize, Serialize};

use std::collections::HashMap;

pub type Vec2 = [f64; 2];
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug)]
pub enum Permission {
    Login,
    GetEntityData,
}

#[derive(Debug)]
pub struct State {
    pub groups: Vec<Group>,
    pub users: Vec<User>,
    pub entities: HashMap<String, Entity>,
}

#[derive(Debug)]
pub struct Group {
    pub id: u32,
    pub name: String,
    pub permissions: Vec<Permission>,
}

#[derive(Debug)]
pub struct User {
    pub id: u32,
    pub username: String,
    pub password: String,
    pub groups: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub position: Vec2,
    pub angle: f32,
}

impl State {
    pub fn new() -> Self {
        State {
            groups: Vec::new(),
            users: Vec::new(),
            entities: HashMap::new(),
        }
    }

    pub fn push_group(&mut self, name: &str, permissions: Vec<Permission>) -> u32 {
        let id = self.groups.len() as u32;
        self.groups.push(Group::new(id, name, permissions));
        id
    }

    pub fn push_user(&mut self, username: &str, password: &str, groups: Vec<u32>) -> u32 {
        let id = self.users.len() as u32;
        self.users.push(User::new(id, username, password, groups));
        id
    }
}

impl Group {
    pub fn new(id: u32, name: &str, permissions: Vec<Permission>) -> Self {
        Group {
            id,
            name: String::from(name),
            permissions,
        }
    }
}

impl User {
    pub fn new(id: u32, username: &str, password: &str, groups: Vec<u32>) -> Self {
        User {
            id,
            username: String::from(username),
            password: String::from(password),
            groups,
        }
    }
}

impl Entity {
    pub fn new(id: &str, position: Vec2, angle: f32) -> Self {
        Entity {
            id: String::from(id),
            position,
            angle,
        }
    }
}
