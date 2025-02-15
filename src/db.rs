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
    InsertTVShows(
        Vec<TVShowListItem>,
        Vec<TVSeasonListItem>,
        Vec<TVEpisodeListItem>,
    ),
    GetTVShowList,
    GetTVSeasons(TVShowListItem),
    GetTVEpisodes(u32, i16),
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
                    Ok::<_, tokio_rusqlite::Error>(())
                })
                .await;

            dbg!(res.err());

            // Now send UpdateServers (hopefully) to let front end know
            get_server_list(conn).await
        }

        SqlCommand::InsertMovies(movies) => insert_movies(conn, movies).await,

        SqlCommand::GetMovieList => get_movie_list(conn).await,

        SqlCommand::InsertTVShows(tvshows, seasons, episodes) => {
            insert_tvshows(conn, tvshows).await?;
            insert_tvseasons(conn, seasons).await?;
            insert_tvepisodes(conn, episodes).await
        }

        SqlCommand::GetTVShowList => get_tv_show_list(conn).await,

        SqlCommand::GetTVSeasons(tvshow) => get_tv_show_seasons(conn, tvshow).await,

        SqlCommand::GetTVEpisodes(tvshow, season) => {
            get_tv_episode_list(conn, tvshow, season).await
        }
    }
}

async fn get_tv_show_list(conn: &Connection) -> Result<Event, tokio_rusqlite::Error> {
    let shows_result = conn
        .call(move |conn| {
            let q = "SELECT * FROM tvshowlist ORDER BY title";
            let mut stmt = conn.prepare(q)?;
            let shows = stmt
                .query_map([], |row| {
                    Ok(TVShowListItem {
                        tvshowid: row.get(0)?,
                        title: row.get(1)?,
                        year: row.get(2)?,
                        season: row.get(3)?,
                        episode: row.get(4)?,
                        file: row.get(5)?,
                        dateadded: row.get(6)?,
                        genre: {
                            let genre_str: String = row.get(7)?;
                            genre_str.split(",").map(String::from).collect()
                        },
                        rating: row.get(8)?,
                        playcount: row.get(9)?,
                        art: Art {
                            poster: {
                                let poster_str: String = row.get(10)?;
                                if poster_str.is_empty() {
                                    None
                                } else {
                                    Some(poster_str)
                                }
                            },
                            thumb: None,
                        },
                    })
                })?
                .collect::<Result<Vec<TVShowListItem>, rusqlite::Error>>()?;
            Ok::<_, tokio_rusqlite::Error>(shows)
        })
        .await;

    match shows_result {
        Ok(shows) => Ok(Event::UpdateTVShowList(shows)),
        Err(err) => {
            dbg!(&err);
            Err(err) // Return the error
        }
    }
}

async fn get_tv_show_seasons(
    conn: &Connection,
    tvshow: TVShowListItem,
) -> Result<Event, tokio_rusqlite::Error> {
    let tvshowid = tvshow.tvshowid;
    let seasons_result = conn
        .call(move |conn| {
            let q = "SELECT * FROM tvseasonlist WHERE tvshowid = ?1 ORDER BY season";
            let mut stmt = conn.prepare(q)?;
            let seasons = stmt
                .query_map([tvshowid], |row| {
                    Ok(TVSeasonListItem {
                        seasonid: row.get(0)?,
                        tvshowid: row.get(1)?,
                        title: row.get(2)?,
                        season: row.get(3)?,
                        episode: row.get(4)?,
                    })
                })?
                .collect::<Result<Vec<TVSeasonListItem>, rusqlite::Error>>()?;
            Ok::<_, tokio_rusqlite::Error>(seasons)
        })
        .await;

    match seasons_result {
        Ok(seasons) => Ok(Event::UpdateTVSeasonList(tvshow, seasons)),
        Err(err) => {
            dbg!(&err);
            Err(err) // Return the error
        }
    }
}

async fn get_tv_episode_list(
    conn: &Connection,
    tvshow: u32,
    season: i16,
) -> Result<Event, tokio_rusqlite::Error> {
    let episodes_result = conn
        .call(move |conn| {
            let (q, params) = if season == -1 {
                (
                    "SELECT * FROM tvepisodelist WHERE tvshowid = ?1
                    ORDER BY 
                        CASE WHEN specialsortseason = -1 THEN season ELSE specialsortseason END ASC,
                        CASE WHEN specialsortepisode = -1 THEN episode ELSE specialsortepisode END ASC;
                    ",
                    params![tvshow],
                )
            } else {
                (
                    "SELECT * FROM tvepisodelist
                    WHERE tvshowid = ?1
                      AND (season = ?2 OR specialsortseason = ?2)
                    ORDER BY
                        CASE WHEN specialsortseason = -1 THEN season ELSE specialsortseason END ASC,
                        CASE WHEN specialsortepisode = -1 THEN episode ELSE specialsortepisode END ASC;",                   
                    params![tvshow, season],
                )
            };

            let mut stmt = conn.prepare(q)?;
            let episodes = stmt
                .query_map(params, |row| {
                    Ok(TVEpisodeListItem {
                        episodeid: row.get(0)?,
                        tvshowid: row.get(1)?,
                        title: row.get(2)?,
                        season: row.get(3)?,
                        episode: row.get(4)?,
                        file: row.get(5)?,
                        dateadded: row.get(6)?,
                        rating: row.get(7)?,
                        firstaired: row.get(8)?,
                        playcount: row.get(9)?,
                        art: Art {
                            poster: None,
                            thumb: {
                                let thumb_str: String = row.get(10)?;
                                if thumb_str.is_empty() {
                                    None
                                } else {
                                    Some(thumb_str)
                                }
                            },
                        },
                        specialsortseason: row.get(11)?,
                        specialsortepisode: row.get(12)?,
                    })
                })?
                .collect::<Result<Vec<TVEpisodeListItem>, rusqlite::Error>>()?;
            Ok::<_, tokio_rusqlite::Error>(episodes)
        })
        .await;

    let tvtitle = conn
        .call(move |conn| {
            let q = "SELECT title FROM tvshowlist WHERE tvshowid = ?1";
            let title = conn.query_row(q, [tvshow], |row| row.get(0))?;
            Ok::<String, tokio_rusqlite::Error>(title)
        })
        .await;

    match episodes_result {
        Ok(episodes) => Ok(Event::UpdateEpisodeList(
            tvtitle.unwrap_or_default(),
            episodes,
        )),
        Err(err) => {
            dbg!(&err);
            Err(err) // Return the error
        }
    }
}

async fn get_movie_list(conn: &Connection) -> Result<Event, tokio_rusqlite::Error> {
    let movies_result = conn
        .call(|conn| {
            let q = "SELECT * FROM movielist ORDER BY dateadded DESC";
            let mut stmt = conn.prepare(q)?;
            let movies = stmt
                .query_map([], |row| {
                    Ok(MovieListItem {
                        movieid: row.get(0)?,
                        title: row.get(1)?,
                        genre: {
                            let genre_str: String = row.get(2)?;
                            genre_str.split(",").map(String::from).collect()
                        },
                        year: row.get(3)?,
                        rating: row.get(4)?,
                        playcount: row.get(5)?,
                        file: row.get(6)?,
                        dateadded: row.get(7)?,
                        premiered: row.get(8)?,
                        art: Art {
                            poster: {
                                let poster_str: String = row.get(9)?;
                                if poster_str.is_empty() {
                                    None
                                } else {
                                    Some(poster_str)
                                }
                            },
                            thumb: None,
                        },
                    })
                })?
                .collect::<Result<Vec<MovieListItem>, rusqlite::Error>>()?;

            Ok::<_, tokio_rusqlite::Error>(movies)
        })
        .await;

    match movies_result {
        Ok(movies) => Ok(Event::UpdateMovieList(movies)),
        Err(err) => {
            dbg!(&err);
            Err(err) // Return the error
        }
    }
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
            Ok::<_, tokio_rusqlite::Error>(servers)
        })
        .await??;
    Ok(Event::UpdateServers(servers))
}

async fn insert_movies(
    conn: &Connection,
    movies: Vec<MovieListItem>,
) -> Result<Event, tokio_rusqlite::Error> {
    // This method does NOT clear old db entries
    // so there may be stale movie references in the DB
    // can likely make a 'clean db' function that takes only movieid list
    let movies_result = conn
        .call(|conn| {
            let t = conn.transaction()?;

            let mut stmt = t.prepare(
                "INSERT OR REPLACE INTO movielist (
                    movieid, title, genre, year, rating, playcount, file, dateadded, premiered, art
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10
                )",
            )?;

            for movie in movies {
                stmt.execute(params![
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
            drop(stmt);

            t.commit()?;
            Ok::<_, tokio_rusqlite::Error>(())
        })
        .await;

    if let Err(err) = movies_result {
        dbg!(&err);
        return Err(err);
    }

    Ok(Event::None)
}

async fn insert_tvshows(
    conn: &Connection,
    tvshows: Vec<TVShowListItem>,
) -> Result<Event, tokio_rusqlite::Error> {
    // This method does NOT clear old db entries
    // so there may be stale references in the DB
    let shows_result = conn
        .call(|conn| {
            let t = conn.transaction()?;

            let mut stmt = t.prepare(
                "INSERT OR REPLACE INTO tvshowlist (
                    tvshowid, title, year, season, episode, file, dateadded, genre, rating, playcount, art
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11
                )",
            )?;

            for tv_show in tvshows {
                stmt.execute(params![
                    tv_show.tvshowid,
                    tv_show.title,
                    tv_show.year,
                    tv_show.season,
                    tv_show.episode,
                    tv_show.file,
                    tv_show.dateadded,
                    tv_show.genre.join(","),
                    tv_show.rating,
                    tv_show.playcount,
                    tv_show.art.poster.unwrap_or("".to_string()),
                ])?;
            }
            drop(stmt);

            t.commit()?;
            Ok::<_, tokio_rusqlite::Error>(())
        })
        .await;

    if let Err(err) = shows_result {
        dbg!(&err);
        return Err(err);
    }

    Ok(Event::None)
}

async fn insert_tvseasons(
    conn: &Connection,
    seasons: Vec<TVSeasonListItem>,
) -> Result<Event, tokio_rusqlite::Error> {
    // This method does NOT clear old db entries
    // so there may be stale references in the DB
    let shows_result = conn
        .call(|conn| {
            let t = conn.transaction()?;

            let mut stmt = t.prepare(
                "INSERT OR REPLACE INTO tvseasonlist (
                    seasonid, tvshowid, title, season, episode
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5
                )",
            )?;

            for season in seasons {
                stmt.execute(params![
                    season.seasonid,
                    season.tvshowid,
                    season.title,
                    season.season,
                    season.episode,
                ])?;
            }
            drop(stmt);

            t.commit()?;
            Ok::<_, tokio_rusqlite::Error>(())
        })
        .await;

    if let Err(err) = shows_result {
        dbg!(&err);
        return Err(err);
    }

    Ok(Event::None)
}

async fn insert_tvepisodes(
    conn: &Connection,
    episodes: Vec<TVEpisodeListItem>,
) -> Result<Event, tokio_rusqlite::Error> {
    // This method does NOT clear old db entries
    // so there may be stale references in the DB
    let shows_result = conn
        .call(|conn| {
            let t = conn.transaction()?;

            let mut stmt = t.prepare(
                "INSERT OR REPLACE INTO tvepisodelist (
                    episodeid, tvshowid, title, season, episode, file, dateadded, rating, 
                    firstaired, playcount, art, specialsortseason, specialsortepisode
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
                )",
            )?;

            for episode in episodes {
                stmt.execute(params![
                    episode.episodeid,
                    episode.tvshowid,
                    episode.title,
                    episode.season,
                    episode.episode,
                    episode.file,
                    episode.dateadded,
                    episode.rating,
                    episode.firstaired,
                    episode.playcount,
                    episode.art.thumb.unwrap_or("".to_string()),
                    episode.specialsortseason,
                    episode.specialsortepisode,
                ])?;
            }
            drop(stmt);

            t.commit()?;
            Ok::<_, tokio_rusqlite::Error>(())
        })
        .await;

    if let Err(err) = shows_result {
        dbg!(&err);
        return Err(err);
    }

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
    Ok::<_, tokio_rusqlite::Error>}(())
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
    Ok::<_, tokio_rusqlite::Error>}(())
    ).await;

    dbg!(movielist.err());

    // Due to websocket response size limits I have to keep the movielist to minimal fields
    // I can create a moviedetails db with the same `movieid` then use JOIN

    let tvshowlist = conn.call(|conn| {conn.execute(
        "CREATE TABLE IF NOT EXISTS 'tvshowlist' (
            tvshowid INTEGER PRIMARY KEY ON CONFLICT REPLACE,
            title TEXT,
            year INTEGER,
            season INTEGER,
            episode INTEGER,
            file TEXT,
            dateadded TEXT,
            genre TEXT,
            rating REAL,
            playcount NUMBER,
            art TEXT
        )",
        [],
    )?;
    Ok::<_, tokio_rusqlite::Error>}(())
    ).await;

    dbg!(tvshowlist.err());

    let tvseasonlist = conn.call(|conn| {conn.execute(
        "CREATE TABLE IF NOT EXISTS 'tvseasonlist' (
            seasonid INTEGER PRIMARY KEY ON CONFLICT REPLACE,
            tvshowid INTEGER,
            title TEXT,
            season INTEGER,
            episode INTEGER
        )",
        [],
    )?;
    Ok::<_, tokio_rusqlite::Error>}(())
    ).await;

    dbg!(tvseasonlist.err());

    let tvepisodelist = conn.call(|conn| {conn.execute(
        "CREATE TABLE IF NOT EXISTS 'tvepisodelist' (
            episodeid INTEGER PRIMARY KEY ON CONFLICT REPLACE,
            tvshowid INTEGER,
            title TEXT,
            season INTEGER,
            episode INTEGER,
            file TEXT,
            dateadded TEXT,
            rating REAL,
            firstaired TEXT,
            playcount NUMBER,
            art TEXT,
            specialsortseason INTEGER,
            specialsortepisode INTEGER
        )",
        [],
    )?;
    Ok::<_, tokio_rusqlite::Error>}(())
    ).await;

    dbg!(tvepisodelist.err());

    Ok(())
}

#[derive(Debug, Clone)]
pub enum Event {
    Opened(SqlConnection),
    Closed,
    None,
    UpdateServers(Vec<KodiServer>),
    UpdateMovieList(Vec<MovieListItem>),
    UpdateTVShowList(Vec<TVShowListItem>),
    UpdateTVSeasonList(TVShowListItem, Vec<TVSeasonListItem>),
    UpdateEpisodeList(String, Vec<TVEpisodeListItem>),
}
