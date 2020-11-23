use crate::types::*;

use nalgebra_glm::Vec3;
use rusqlite::{named_params, Connection};
use tracing::info;

const DB_PATH: &str = "ServerDB.db";

#[derive(Clone, Debug)]
pub struct UserDetails {
    username: String,
    id: i32,
}

pub fn authenticate_user(username: &str, password: &str) -> Result<UserDetails> {
    let db = Connection::open(&DB_PATH)?;
    info!("Connected to database {:?}", DB_PATH);

    let mut stmt = db.prepare(
        "select id
        from users
        where username = :username and
              password = :password;",
    )?;

    let mut rows = stmt.query_named(named_params! {
        ":username": &username,
        ":password": &password
    })?;

    let id: i32 = rows.next()?.ok_or("Invalid credentials")?.get(0)?;
    info!("Found user {} id {}", username, id);

    Ok(UserDetails {
        username: username.to_owned(),
        id: id,
    })
}

pub fn get_last_player_position(username: &str) -> Result<Vec3> {
    let db = Connection::open(&DB_PATH)?;
    info!("Connected to database {:?}", DB_PATH);

    let mut stmt = db.prepare(
        "select long, alt, lat
        from users
        where username = :username;",
    )?;

    let mut rows = stmt.query_named(named_params! {
        ":username": &username,
    })?;

    let row = rows.next()?.ok_or("Invalid credentials")?;
    let x: f64 = row.get(0)?;
    let y: f64 = row.get(1)?;
    let z: f64 = row.get(2)?;
    let position = Vec3::new(x as f32, y as f32, z as f32);

    info!("Got player {} position {:?}", username, position);

    Ok(position)
}

pub fn set_last_player_position(username: &str, position: Vec3) -> Result<()> {
    let db = Connection::open(&DB_PATH)?;
    info!("Connected to database {:?}", DB_PATH);

    let mut stmt = db.prepare(
        "update users
        set long = :x,
            alt = :y,
            lat = :z
        where username = :username;",
    )?;

    stmt.execute_named(named_params! {
        ":x": &(position.x as f64),
        ":y": &(position.y as f64),
        ":z": &(position.z as f64),
        ":username": &username,
    })?;

    Ok(())
}
