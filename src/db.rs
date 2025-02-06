use iced::futures::channel::mpsc::{channel, Receiver, Sender};
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::stream;

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

pub fn connect() -> impl Stream<Item = Event> {
    // struct Conn;

    stream::channel(100, |output| async move { handle_connection(output).await })
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
                            panic!("Sqlite err creating tables");
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

async fn handle_command(
    cmd: SqlCommand,
    conn: &mut Connection,
) -> Result<Event, tokio_rusqlite::Error> {
    match cmd {
        SqlCommand::GetServers => get_server_list(conn).await,

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
            get_server_list(conn).await
        }

        SqlCommand::InsertMovies(movies) => insert_movies(conn, movies).await,

        SqlCommand::GetMovieList => get_movie_list(conn).await,
    }
}

async fn get_movie_list(conn: &Connection) -> Result<Event, tokio_rusqlite::Error> {
    let movies = conn
        .call(|conn| {
            let q = "SELECT * FROM movielist ORDER BY dateadded DESC";
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
        .await??;
    Ok(Event::UpdateMovieList(movies))
}

async fn get_server_list(conn: &Connection) -> Result<Event, tokio_rusqlite::Error> {
    let servers = conn
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
        .await??;
    Ok(Event::UpdateServers(servers))
}

async fn insert_movies(
    conn: &Connection,
    movies: Vec<MovieListItem>,
) -> Result<Event, tokio_rusqlite::Error> {
    let movies = conn.call(|conn| {
        // !TODO! 
        // This just clears the whole table and re-inserts all entries currently
        // it could be done much more efficiently I think?

        let t = conn.transaction()?;
        t.execute("DELETE FROM movielist", [])?;
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
    Ok(Event::None)
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
    // Will eventually be used for selected server and others.
    // sort options (movie/files/etc)
    // let settings = conn.call(|conn| {conn.execute(
    //     "CREATE TABLE IF NOT EXISTS 'settings' (
    //         setting TEXT PRIMARY KEY ON CONFLICT REPLACE,
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

    // Due to websocket response size limits I have to keep the movielist to minimal fields
    // I can create a moviedetails db with the same `movieid` then use JOIN

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
