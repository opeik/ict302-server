use crate::types::*;

use lazy_static::lazy_static;
use rusqlite::{named_params, params, Connection, Row, NO_PARAMS};
use tracing::{debug, error, info, warn};

use std::path::Path;

const DB_PATH: &str = "ServerDB.db";

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
