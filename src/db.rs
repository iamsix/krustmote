use std::path::Path;

use iced::futures::StreamExt;
use iced::futures::channel::mpsc::{Receiver, Sender, channel};
use iced::futures::channel::oneshot;

use anyhow::{Context, Result};
use tokio::fs;
use tokio_rusqlite::Connection;
use tokio_rusqlite::params;
use tracing::{debug, error};

use crate::koditypes::*;

#[derive(Debug)]
pub enum SqlCommand {
    GetServers {
        sender: oneshot::Sender<Vec<KodiServer>>,
    },
    AddOrEditServer(KodiServer),

    GetMovieList {
        sender: oneshot::Sender<Vec<Box<dyn IntoListData + Send>>>,
    },
    GetTVShowList {
        sender: oneshot::Sender<Vec<Box<dyn IntoListData + Send>>>,
    },
    GetTVSeasons {
        sender: oneshot::Sender<Vec<Box<dyn IntoListData + Send>>>,
        tvshowid: u32,
    },
    GetTVEpisodes {
        sender: oneshot::Sender<Vec<Box<dyn IntoListData + Send>>>,
        tvshowid: u32,
        season: i16,
    },
    GetTVShowItem {
        sender: oneshot::Sender<TVShowListItem>,
        tvshowid: u32,
    },

    InsertMovies(Vec<MovieListItem>), // bool clear_before_insert?
    InsertTVShows {
        tvshows: Vec<TVShowListItem>,
        do_clean: bool,
    }, // same
    InsertTVSeasons(Vec<TVSeasonListItem>, u32),
    InsertTVEpisodes(Vec<TVEpisodeListItem>, u32), // same

    // ID-based sync operations
    GetMovieIDs {
        sender: oneshot::Sender<Vec<u32>>,
    },
    GetTVShowIDs {
        sender: oneshot::Sender<Vec<u32>>,
    },
    GetTVEpisodeIDs {
        sender: oneshot::Sender<Vec<u32>>,
        tvshowid: u32,
    },
    DeleteMoviesByIDs(Vec<u32>),
    DeleteTVShowsByIDs(Vec<u32>),
    DeleteTVEpisodesByIDs {
        ids: Vec<u32>,
        tvshowid: u32,
    },
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

pub async fn connect(output: oneshot::Sender<SqlConnection>) {
    let dir = crate::PROJECT_DIRS.config_dir();
    let db_path = dir.join("krustmote.db");
    let path = if fs::metadata(&dir).await.is_ok() {
        db_path
    } else {
        if fs::create_dir_all(&dir).await.is_ok() {
            db_path
        } else {
            Path::new("./krustmote.db").to_path_buf()
        }
    };
    match Connection::open(path).await {
        Ok(conn) => {
            let res = create_tables(&conn).await;
            if res.is_err() {
                error!("Sqlite err creating tables: {:?}", res.err());
                panic!("Sqlite err creating tables");
            }

            let (sender, reciever) = channel(100);
            let _ = output.send(SqlConnection(sender));

            handle_connection(conn, reciever).await;
        }
        Err(err) => {
            // let _ = output;
            error!("Failed to open database: {:?}", err);
        }
    }
}

async fn handle_connection(mut conn: Connection, mut reciever: Receiver<SqlCommand>) -> ! {
    loop {
        let cmd = reciever.select_next_some().await;
        let res = handle_command(cmd, &mut conn).await;
        if res.is_err() {
            error!("Database command error: {:?}", res.err());
        }
    }
}

async fn handle_command(cmd: SqlCommand, conn: &mut Connection) -> Result<()> {
    match cmd {
        SqlCommand::GetServers { sender } => get_server_list(conn, sender).await,

        SqlCommand::AddOrEditServer(server) => {
            // NOTE might change this to NOT return the servers
            debug!(?server, "Add or Edit server");
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

            if let Err(err) = res {
                error!("Failed to insert/replace server: {:?}", err);
            }
            Ok(())
        }

        SqlCommand::InsertMovies(movies) => insert_movies(conn, movies).await,

        SqlCommand::InsertTVShows { tvshows, do_clean } => {
            insert_tvshows(conn, tvshows, do_clean).await
        }

        SqlCommand::InsertTVSeasons(seasons, tvshowid) => {
            insert_tvseasons(conn, seasons, tvshowid).await
        }

        SqlCommand::InsertTVEpisodes(episodes, tvshowid) => {
            insert_tvepisodes(conn, episodes, tvshowid).await
        }

        SqlCommand::GetMovieList { sender } => get_movie_list(conn, sender).await,

        SqlCommand::GetTVShowList { sender } => get_tv_show_list(conn, sender).await,

        SqlCommand::GetTVSeasons { sender, tvshowid } => {
            get_tv_seasons_list(conn, sender, tvshowid).await
        }

        SqlCommand::GetTVEpisodes {
            sender,
            tvshowid,
            season,
        } => get_tv_episode_list(conn, sender, tvshowid, season).await,

        SqlCommand::GetTVShowItem { sender, tvshowid } => {
            get_tv_show_item(conn, sender, tvshowid).await
        }

        SqlCommand::GetMovieIDs { sender } => get_movie_ids(conn, sender).await,
        SqlCommand::GetTVShowIDs { sender } => get_tvshow_ids(conn, sender).await,
        SqlCommand::GetTVEpisodeIDs { sender, tvshowid } => {
            get_tvepisode_ids(conn, sender, tvshowid).await
        }

        SqlCommand::DeleteMoviesByIDs(ids) => delete_movies_by_ids(conn, ids).await,
        SqlCommand::DeleteTVShowsByIDs(ids) => delete_tvshows_by_ids(conn, ids).await,
        SqlCommand::DeleteTVEpisodesByIDs { ids, tvshowid } => {
            delete_tvepisodes_by_ids(conn, ids, tvshowid).await
        }
    }
}

// note I may add a limiter/condition to tv_show_list later instead of this
async fn get_tv_show_item(
    conn: &Connection,
    sender: oneshot::Sender<TVShowListItem>,
    tvshowid: u32,
) -> Result<()> {
    let item_result = conn
        .call(move |conn| {
            let q = "SELECT * FROM tvshowlist WHERE tvshowid = ?1";
            let item = conn.query_row(q, [tvshowid], |row| {
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
            })?;
            Ok::<TVShowListItem, tokio_rusqlite::Error>(item)
        })
        .await?;
    let _ = sender.send(item_result);

    Ok(())
}

// for all the <media>_list functions boxing directly during query_map
//   seems to be a good perf/efficiency improvement.
async fn get_tv_show_list(
    conn: &Connection,
    sender: oneshot::Sender<Vec<Box<dyn IntoListData + Send>>>,
) -> Result<()> {
    let shows_result = conn
        .call(move |conn| {
            let q = "SELECT * FROM tvshowlist ORDER BY title COLLATE NOCASE ASC";
            let mut stmt = conn.prepare(q)?;
            let shows = stmt
                .query_map([], |row| {
                    Ok(Box::new(TVShowListItem {
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
                    }) as _)
                })?
                .collect::<Result<Vec<Box<dyn IntoListData + Send>>, rusqlite::Error>>()?;
            Ok::<_, tokio_rusqlite::Error>(shows)
        })
        .await?;

    let _ = sender.send(shows_result);
    Ok(())
}

async fn get_tv_seasons_list(
    conn: &Connection,
    sender: oneshot::Sender<Vec<Box<dyn IntoListData + Send>>>,
    tvshowid: u32,
) -> Result<()> {
    let seasons_result = conn
        .call(move |conn| {
            let q = "SELECT * FROM tvseasonlist WHERE tvshowid = ?1 ORDER BY season";
            let mut stmt = conn.prepare(q)?;
            let seasons = stmt
                .query_map([tvshowid], |row| {
                    Ok(Box::new(TVSeasonListItem {
                        seasonid: row.get(0)?,
                        tvshowid: row.get(1)?,
                        title: row.get(2)?,
                        season: row.get(3)?,
                        episode: row.get(4)?,
                    }) as _)
                })?
                .collect::<Result<Vec<Box<dyn IntoListData + Send>>, rusqlite::Error>>()?;
            Ok::<_, tokio_rusqlite::Error>(seasons)
        })
        .await?;

    let _ = sender.send(seasons_result);
    Ok(())
}

async fn get_tv_episode_list(
    conn: &Connection,
    sender: oneshot::Sender<Vec<Box<dyn IntoListData + Send>>>,
    tvshow: u32,
    season: i16,
) -> Result<()> {
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
                    Ok(Box::new(TVEpisodeListItem {
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
                    }) as _)
                })?
                .collect::<Result<Vec<Box<dyn IntoListData + Send>>, rusqlite::Error>>()?;
            Ok::<_, tokio_rusqlite::Error>(episodes)
        })
        .await?;

    let _ = sender.send(episodes_result);
    Ok(())
}

async fn get_movie_list(
    conn: &Connection,
    sender: oneshot::Sender<Vec<Box<dyn IntoListData + Send>>>,
) -> Result<()> {
    let movies_result = conn
        .call(|conn| {
            let q = "SELECT * FROM movielist ORDER BY dateadded DESC";
            let mut stmt = conn.prepare(q)?;
            let movies = stmt
                .query_map([], |row| {
                    Ok(Box::new(MovieListItem {
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
                    }) as _)
                })?
                .collect::<Result<Vec<Box<dyn IntoListData + Send>>, rusqlite::Error>>()?;

            Ok::<_, tokio_rusqlite::Error>(movies)
        })
        .await?;

    let _ = sender.send(movies_result);
    Ok(())
}

async fn get_server_list(
    conn: &Connection,
    sender: oneshot::Sender<Vec<KodiServer>>,
) -> Result<()> {
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
    let _ = sender.send(servers);
    Ok(())
}

async fn insert_movies(conn: &Connection, movies: Vec<MovieListItem>) -> Result<()> {
    conn.call(|conn| {
        let movie_ids: Vec<u32> = movies.iter().map(|e| e.movieid).collect();
        let min_dateadded = movies.iter().map(|e| e.dateadded.clone()).min().unwrap();

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

        // clean stale entries
        if !min_dateadded.is_empty() {
            t.execute(
                "CREATE TEMP TABLE temp_movie_ids (movieid INTEGER PRIMARY KEY)",
                [],
            )?;
            let mut temp_insert = t.prepare(
                "INSERT INTO temp_movie_ids 
                 (movieid) VALUES (?)",
            )?;
            for movie_id in &movie_ids {
                temp_insert.execute(params![movie_id])?;
            }
            drop(temp_insert);

            // Delete using a JOIN with the temporary table
            let delete_sql = "DELETE FROM movielist WHERE 
            movieid NOT IN (SELECT movieid FROM temp_movie_ids) 
            AND dateadded >= ?";
            let mut delete_stmt = t.prepare(&delete_sql)?;
            delete_stmt.execute(params![min_dateadded])?;
            drop(delete_stmt);

            t.execute("DROP TABLE temp_movie_ids", [])?;
        }

        t.commit()?;
        Ok::<_, tokio_rusqlite::Error>(())
    })
    .await
    .context("Failed to insert movies DB")?;

    Ok(())
}

async fn insert_tvshows(
    conn: &Connection,
    tvshows: Vec<TVShowListItem>,
    do_clean: bool,
) -> Result<()> {
    conn.call(move |conn| {
        let tvshow_ids: Vec<u32> = tvshows.iter().map(|e| e.tvshowid).collect();
        let min_dateadded = tvshows.iter().map(|e| e.dateadded.clone()).min().unwrap();

        let t = conn.transaction()?;

        let mut stmt = t.prepare(
            "INSERT OR REPLACE INTO tvshowlist (
                tvshowid, title, year, season, episode, file, dateadded, genre, rating, playcount, art
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11
            )",
        )?;

        for tv_show in tvshows {
            // for now we don't accept any show without a dateadded
            // Note blank dateadded *should* never happen 
            // but can (on tvshows with no episodes?) due to weird kodi db things.
            if tv_show.dateadded.is_empty() {
                continue;
            }
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

        // clean stale entries
        if !min_dateadded.is_empty() && do_clean{
            t.execute(
                "CREATE TEMP TABLE temp_tvshow_ids (tvshowid INTEGER PRIMARY KEY)",
                [],
            )?;
            let mut temp_insert = t.prepare(
                "INSERT INTO temp_tvshow_ids 
                    (tvshowid) VALUES (?)",
            )?;
            for tvshow_id in &tvshow_ids {
                temp_insert.execute(params![tvshow_id])?;
            }
            drop(temp_insert);

            // Delete using a JOIN with the temporary table
            let delete_sql = "DELETE FROM tvshowlist WHERE 
                tvshowid NOT IN (SELECT tvshowid FROM temp_tvshow_ids) 
                AND dateadded >= ?";
            let mut delete_stmt = t.prepare(&delete_sql)?;
            delete_stmt.execute(params![min_dateadded])?;
            drop(delete_stmt);

            t.execute("DROP TABLE temp_tvshow_ids", [])?;
        } else {
            if do_clean {
                dbg!("Empty dateadded entry found.");
            }
        }

        t.commit()?;
        Ok::<_, tokio_rusqlite::Error>(())
    })
    .await
    .context("Failed to insert tvshows DB")?;

    Ok(())
}

async fn insert_tvseasons(
    conn: &Connection,
    seasons: Vec<TVSeasonListItem>,
    tvshowid: u32,
) -> Result<()> {
    // no need to intelligently clean this one
    // since it always inserts a full season list per show just remove and re-insert
    let shows_result = conn
        .call(move |conn| {
            let t = conn.transaction()?;

            t.execute("DELETE FROM tvseasonlist WHERE tvshowid = ?", [tvshowid])?;

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
        error!("Failed to insert TV seasons: {:?}", err);
        // return Err(err);
    }

    Ok(())
}

async fn insert_tvepisodes(
    conn: &Connection,
    episodes: Vec<TVEpisodeListItem>,
    tvshowid: u32,
) -> Result<()> {
    conn.call(move |conn| {
        let episode_ids: Vec<u32> = episodes.iter().map(|e| e.episodeid).collect();
        let min_dateadded = episodes.iter().map(|e| e.dateadded.clone()).min().unwrap();

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

        // Clean stale entries
        if !min_dateadded.is_empty() {
            t.execute(
                "CREATE TEMP TABLE temp_episode_ids (episodeid INTEGER PRIMARY KEY)",
                [],
            )?;
            let mut temp_insert = t.prepare(
                "INSERT INTO temp_episode_ids 
                 (episodeid) VALUES (?)",
            )?;
            for episode_id in &episode_ids {
                temp_insert.execute(params![episode_id])?;
            }
            drop(temp_insert);

            // Delete using a JOIN with the temporary table
            let delete_sql = "DELETE FROM tvepisodelist WHERE 
            episodeid NOT IN (SELECT episodeid FROM temp_episode_ids) 
            AND dateadded >= ?1 AND tvshowid = ?2";
            let mut delete_stmt = t.prepare(&delete_sql)?;
            delete_stmt.execute(params![min_dateadded, tvshowid])?;
            drop(delete_stmt);

            t.execute("DROP TABLE temp_episode_ids", [])?;
        }

        t.commit()?;
        Ok::<_, tokio_rusqlite::Error>(())
    })
    .await
    .context("Failed to insert episodes DB")?;

    Ok(())
}

// ID-based sync operations
async fn get_movie_ids(conn: &Connection, sender: oneshot::Sender<Vec<u32>>) -> Result<()> {
    let ids = conn
        .call(|conn| {
            let q = "SELECT movieid FROM movielist";
            let mut stmt = conn.prepare(q)?;
            let ids = stmt
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<u32>, rusqlite::Error>>()?;
            Ok::<Vec<u32>, tokio_rusqlite::Error>(ids)
        })
        .await?;
    let _ = sender.send(ids);
    Ok(())
}

async fn get_tvshow_ids(conn: &Connection, sender: oneshot::Sender<Vec<u32>>) -> Result<()> {
    let ids = conn
        .call(|conn| {
            let q = "SELECT tvshowid FROM tvshowlist";
            let mut stmt = conn.prepare(q)?;
            let ids = stmt
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<u32>, rusqlite::Error>>()?;
            Ok::<Vec<u32>, tokio_rusqlite::Error>(ids)
        })
        .await?;
    let _ = sender.send(ids);
    Ok(())
}

async fn get_tvepisode_ids(
    conn: &Connection,
    sender: oneshot::Sender<Vec<u32>>,
    tvshowid: u32,
) -> Result<()> {
    let ids = conn
        .call(move |conn| {
            let q = "SELECT episodeid FROM tvepisodelist WHERE tvshowid = ?1";
            let mut stmt = conn.prepare(q)?;
            let ids = stmt
                .query_map([tvshowid], |row| row.get(0))?
                .collect::<Result<Vec<u32>, rusqlite::Error>>()?;
            Ok::<Vec<u32>, tokio_rusqlite::Error>(ids)
        })
        .await?;
    let _ = sender.send(ids);
    Ok(())
}

async fn delete_movies_by_ids(conn: &Connection, ids: Vec<u32>) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }

    conn.call(move |conn| {
        let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let q = format!("DELETE FROM movielist WHERE movieid IN ({})", placeholders);
        let mut stmt = conn.prepare(&q)?;

        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        stmt.execute(params.as_slice())?;
        Ok::<_, tokio_rusqlite::Error>(())
    })
    .await?;

    Ok(())
}

async fn delete_tvshows_by_ids(conn: &Connection, ids: Vec<u32>) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }

    conn.call(move |conn| {
        let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let q = format!(
            "DELETE FROM tvshowlist WHERE tvshowid IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&q)?;

        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        stmt.execute(params.as_slice())?;
        Ok::<_, tokio_rusqlite::Error>(())
    })
    .await?;

    Ok(())
}

async fn delete_tvepisodes_by_ids(conn: &Connection, ids: Vec<u32>, tvshowid: u32) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }

    conn.call(move |conn| {
        let placeholders = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let q = format!(
            "DELETE FROM tvepisodelist WHERE tvshowid = ? AND episodeid IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&q)?;

        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&tvshowid];
        let id_params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        params.extend(id_params);

        stmt.execute(params.as_slice())?;
        Ok::<_, tokio_rusqlite::Error>(())
    })
    .await?;

    Ok(())
}

async fn create_tables(conn: &Connection) -> Result<()> {
    conn.call(|conn| {conn.execute(
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
    ).await?;

    // dbg!(servers.err());

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

    // TODO - these table names should include db_id ie. movielist0 etc.
    //        or I can make db0.sqlite etc separate from settings/server db

    conn.call(|conn| {conn.execute(
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
    ).await?;

    // dbg!(movielist.err());

    // Due to websocket response size limits I have to keep the movielist to minimal fields
    // I can create a moviedetails db with the same `movieid` then use JOIN

    conn.call(|conn| {conn.execute(
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
    ).await?;

    // dbg!(tvshowlist.err());

    conn.call(|conn| {conn.execute(
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
    ).await?;

    // dbg!(tvseasonlist.err());

    conn.call(|conn| {conn.execute(
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
    ).await?;

    // dbg!(tvepisodelist.err());

    Ok(())
}
