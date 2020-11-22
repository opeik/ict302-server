use crate::types::*;

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
