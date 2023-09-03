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
    InsertMovies(Vec<MovieListItem>),
    GetMovieList,
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
                // TODO! Proper path support
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

        SqlCommand::InsertMovies(movies) => {
            let res = insert_movies(conn, movies).await;
            dbg!(res.err());
            Ok(Event::None)
        }

        SqlCommand::GetMovieList => {
            let res = get_movie_list(conn).await;
            if let Ok(res) = res {
                //dbg!(res);
                Ok(Event::UpdateMovieList(res))
            } else {
                dbg!(res.err());
                Ok(Event::None)
            }
        }
    }
}

// TODO Change this (and get_get_server_list to return an event directly)
async fn get_movie_list(conn: &Connection) -> Result<Vec<MovieListItem>, tokio_rusqlite::Error> {
    Ok(conn
        .call(|conn| {
            let q = "SELECT * FROM movielist";
            let mut stmt = conn.prepare(q)?;
            let movies = stmt
                .query_map([], |row| {
                    let genres: Vec<String> = row
                        .get::<usize, String>(2)?
                        .split(",")
                        .map(|s| s.to_string())
                        .collect();
                    let poster = row.get::<usize, String>(9)?;
                    let poster = if !poster.is_empty() {
                        Some(poster)
                    } else {
                        None
                    };
                    Ok(MovieListItem {
                        movieid: row.get(0)?,
                        title: row.get(1)?,
                        genre: genres,
                        year: row.get(3)?,
                        rating: row.get(4)?,
                        playcount: row.get(5)?,
                        file: row.get(6)?,
                        dateadded: row.get(7)?,
                        premiered: row.get(8)?,
                        art: Art {
                            poster,
                            thumb: None,
                        },
                    })
                })?
                .collect::<Result<Vec<MovieListItem>, rusqlite::Error>>();
            Ok::<_, rusqlite::Error>(movies)
        })
        .await??)
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

async fn insert_movies(
    conn: &Connection,
    movies: Vec<MovieListItem>,
) -> Result<(), rusqlite::Error> {
    let movies = conn.call(|conn| {

        let t = conn.transaction()?;
        for movie in movies {
            t.execute("INSERT INTO movielist VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10
            )", params![
                movie.movieid,
                movie.title,
                movie.genre.join(","),
                movie.year,
                movie.rating,
                movie.playcount,
                movie.file,
                movie.dateadded,
                movie.premiered,
                movie.art.poster.unwrap_or("".to_string()),
             ])?;
        }

        t.commit()?;
        Ok::<_, rusqlite::Error>}(())
    ).await;

    dbg!(movies.err());
    Ok(())
}

async fn create_tables(conn: &Connection) -> Result<(), rusqlite::Error> {
    let servers = conn.call(|conn| {conn.execute(
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

    dbg!(servers.err());

    // Not used yet:
    // Will eventually be used for selected server etc.
    // This technically doesn't need an ID but apparently it's
    // best if it has one for some sql stuff?
    // let settings = conn.call(|conn| {conn.execute(
    //     "CREATE TABLE IF NOT EXISTS 'settings' (
    //         setting TEXT UNIQUE,
    //         value TEXT,
    //     )",
    //     [],
    // )?;
    // Ok::<_, rusqlite::Error>}(())
    // ).await;

    // dbg!(settings.err());

    let movielist = conn.call(|conn| {conn.execute(
        "CREATE TABLE IF NOT EXISTS 'movielist' (
            movieid INTEGER PRIMARY KEY ON CONFLICT REPLACE,
            title TEXT,
            genre TEXT,
            year INTEGER,
            rating REAL,
            playcount NUMBER,
            file TEXT,
            dateadded TEXT,            
            premiered TEXT,
            art TEXT
        )",
        [],
    )?;
    Ok::<_, rusqlite::Error>}(())
    ).await;

    dbg!(movielist.err());

    // Due to websocket response size limits
    // I have to keep the movielist to minimal fields
    // then I can create a moviedetails db with the same movieID and Join
    //

    // imagecache DB? keep the 'url' from kodi as key, and blob as value?

    Ok(())
}

#[derive(Debug, Clone)]
pub enum Event {
    Opened(SqlConnection),
    Closed,
    None,
    UpdateServers(Vec<KodiServer>),
    UpdateMovieList(Vec<MovieListItem>),
}
