mod math;
mod rules;
mod api;
use rusqlite::{Connection, Result};

fn main() -> Result<()> {
    let conn = Connection::open("astrology_journal.db")?;
    let mut stmt = conn.prepare("SELECT name, status FROM Clients")?;
    let clients = stmt.query_map([], |row| {
        Ok(format!("{}: {}", row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    println!("--- Current Clients ---");
    for client in clients {
        println!("{}", client?);
    }

    let mut stmt = conn.prepare("SELECT id, client_id, question FROM Readings")?;
    let readings = stmt.query_map([], |row| {
        Ok(format!("ID: {}, ClientID: {}, Q: {}", row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
    })?;

    println!("\n--- Current Readings ---");
    for reading in readings {
        println!("{}", reading?);
    }
    Ok(())
}
