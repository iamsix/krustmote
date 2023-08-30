use iced::futures::channel::mpsc::{channel, Receiver, Sender};
use iced::futures::{SinkExt, StreamExt};
use iced::subscription::{self, Subscription};

use rusqlite::params;
use tokio_rusqlite::Connection;

use crate::koditypes::*;

#[derive(Debug, Clone)]
pub enum SqlCommand {
    GetServers,
    AddOrEditServer(KodiServer),
}

enum State {
    Closed,
    Open(Connection, Receiver<SqlCommand>),
}

#[derive(Debug, Clone)]
pub struct SqlConnection(Sender<SqlCommand>);
impl SqlConnection {
    pub fn send(&mut self, message: SqlCommand) {
        self.0
            .try_send(message)
            .expect("Should be able to send to sqlite client");
    }
}

pub fn connect() -> Subscription<Event> {
    struct Conn;

    subscription::channel(std::any::TypeId::of::<Conn>(), 100, |output| async move {
        handle_connection(output).await
    })
}

async fn handle_connection(mut output: Sender<Event>) -> ! {
    let mut state = State::Closed;
    loop {
        match &mut state {
            State::Closed => {
                match Connection::open("./krustmote.db").await {
                    Ok(conn) => {
                        let res = create_tables(&conn).await;
                        if res.is_err() {
                            dbg!(res.err());
                            panic!("Sqlite err");
                        }

                        let (sender, reciever) = channel(100);
                        let _ = output.send(Event::Opened(SqlConnection(sender))).await;
                        state = State::Open(conn, reciever);
                    }
                    Err(err) => {
                        let _ = output.send(Event::Closed);
                        dbg!(err);
                    }
                };
            }
            State::Open(conn, input) => {
                let cmd = input.select_next_some().await;
                let res = handle_command(cmd, conn).await;
                if let Ok(res) = res {
                    let _ = output.send(res).await;
                } else {
                    dbg!(res.err());
                }
            }
        }
    }
}

async fn handle_command(cmd: SqlCommand, conn: &mut Connection) -> Result<Event, rusqlite::Error> {
    match cmd {
        SqlCommand::GetServers => match get_server_list(conn).await {
            Ok(servers) => {
                let cmd = Event::UpdateServers(servers);
                Ok(cmd)
            }
            Err(err) => {
                dbg!(err);
                Ok(Event::None)
            }
        },
        SqlCommand::AddOrEditServer(server) => {
            dbg!(&server);
            let res = conn
                .call(move |conn| {
                    let q =
                        "INSERT OR REPLACE INTO servers VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

                    conn.execute(
                        q,
                        params![
                            server.id,
                            server.name,
                            server.ip,
                            server.webserver_port,
                            server.websocket_port,
                            server.username,
                            server.password,
                            server.db_id
                        ],
                    )?;
                    Ok::<_, rusqlite::Error>(())
                })
                .await;

            dbg!(res.err());

            // Now send UpdateServers (hopefully) to let front end know
            match get_server_list(conn).await {
                Ok(servers) => {
                    let cmd = Event::UpdateServers(servers);
                    Ok(cmd)
                }
                Err(err) => {
                    dbg!(err);
                    Ok(Event::None)
                }
            }
        }
    }
}

async fn get_server_list(conn: &Connection) -> Result<Vec<KodiServer>, tokio_rusqlite::Error> {
    Ok(conn
        .call(|conn| {
            let q = "SELECT * FROM servers";
            let mut stmt = conn.prepare(q)?;
            let servers = stmt
                .query_map([], |row| {
                    Ok(KodiServer {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        ip: row.get(2)?,
                        webserver_port: row.get(3)?,
                        websocket_port: row.get(4)?,
                        username: row.get(5)?,
                        password: row.get(6)?,
                        db_id: row.get(7)?,
                    })
                })?
                .collect::<Result<Vec<KodiServer>, rusqlite::Error>>();
            Ok::<_, rusqlite::Error>(servers)
        })
        .await??)
}

async fn create_tables(conn: &Connection) -> Result<(), rusqlite::Error> {
    let x = conn.call(|conn| {conn.execute(
        "CREATE TABLE IF NOT EXISTS 'servers' (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            ip TEXT NOT NULL,
            webserver_port INTEGER NOT NULL,
            websocket_port INTEGER NOT NULL,
            username TEXT,
            password TEXT,
            db_id INTEGER
        )",
        [],
    )?;
    Ok::<_, rusqlite::Error>}(())
    ).await;

    dbg!(x.err());

    // 'settings' (selected_server, various user configable stuff....)
    // this will be a 'key | value' pair database to expand as needed. (key UNIQ)

    // not sure if 'videos' or 'movies' / 'episodes'
    // 'tvshows' table probably requred in either case.
    //     technically could store tvshow name in each entry but seems redundant.
    // id, videotype(?), filepath, title, year, rating, playcount, thumbnail BLOB
    // id, tvseriesid, episode, season, epname, filepath, rating, playcount, thumbnail BLOB
    // the generic videos is *probably* ideal here and just leave them null
    //    for the types that don't make sense (ie no ep number for movies etc)
    // might have to be videos_<server_id> etc to keep them separate?
    // note the kodi methods are GetMovies GetEpisodes etc so might have to separate due to that
    // use kodi's IDs for id.

    Ok(())
}

#[derive(Debug, Clone)]
pub enum Event {
    Opened(SqlConnection),
    Closed,
    None,
    UpdateServers(Vec<KodiServer>),
}
