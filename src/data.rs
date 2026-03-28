// Front end really only recieves ListData + kodistatus
//   Note ListData can be from any data source, it's just a list
// Front end can send kodireq for remote controls (need to clone producer?)
// Front end decides *what* to request but not how.
//
// Example:
// DataLayer::GetTVShows
//   Checks if the data is in DB
//     If it's there it returns that to UI but also checks if data is current
//        If it's not current it pulls new data and inserts to DB
//        Possibly sends UI an udpdate?
//          maybe check first, update DB if necessary then return always?
//     if not in DB then it then pulls data from kodi
//        pushes data to DB then returns data to UI.

use std::sync::Arc;

use crate::client;
use crate::client::Event;
use crate::db;
use crate::koditypes::*;
use iced::futures::channel::mpsc::{Receiver, Sender, channel};
use iced::futures::channel::oneshot;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::stream;
use std::error::Error;
use tokio::select;
use tracing::{debug, error, info};

// input messages from UI
#[derive(Debug, Clone)]
pub enum Get {
    KodiServers,
    AddOrEditServer(KodiServer),
    Movies,
    TVShows,
    TVSeasons(u32),
    TVEpisodes(u32, i16),
    Directory { path: String, media_type: MediaType },
    Sources,
}

#[derive(Debug, Clone)]
pub struct Connection(Sender<Get>);

impl Connection {
    pub fn send(&mut self, message: Get) {
        self.0
            .try_send(message)
            .expect("Should be able to send to kodi client");
    }
}

//output messages to UI
#[derive(Debug, Clone)]
pub enum DataEvent {
    Offline(Connection),
    Online(Connection, client::Connection),
    ListData {
        title: String,
        data: Vec<Box<dyn IntoListData + Send + 'static>>,
    },
    // technically Servers is redundant
    // can just udpate KodiStatus instead
    Servers(Vec<KodiServer>),
    KodiStatus(crate::KodiStatus),
    InputRequested(String),
}

pub struct Data {
    // I'm not sure I like this thing keeping kodi_status itself.
    // Might turn it in to a mutex or rwlock
    // (front end really only need 'read' from it anyway)
    // biggest hurdle to that is actually the slider grab thing
    // I'd need to decouple that part of the UI during grab.
    kodi_status: crate::KodiStatus,
    db: db::SqlConnection,
    kodi_connected: bool,
    client: client::Connection,
    clientrx: Receiver<client::Event>,
}

pub fn connect() -> impl Stream<Item = DataEvent> {
    stream::channel(100, |output| async move {
        let mut data = Data::new().await;
        data.handle_connection(output).await
    })
}

impl Data {
    pub async fn new() -> Self {
        match Self::initialize_data().await {
            Ok(data) => data,
            Err(err) => {
                error!("Data initialization failed: {:?}", err);
                panic!("Failed")
            }
        }
    }

    async fn initialize_data() -> Result<Data, Box<dyn Error + Send + Sync>> {
        let (dbtx, dbrx) = oneshot::channel();
        tokio::spawn(async move {
            db::connect(dbtx).await;
        });
        let mut conn = dbrx.await?;

        let (tx, rx) = oneshot::channel();
        let _ = conn.send(db::SqlCommand::GetServers { sender: tx });

        let kodiserver = rx.await?;
        let kodiserver = if kodiserver.is_empty() {
            Arc::new(KodiServer::default())
        } else {
            Arc::new(kodiserver[0].clone())
        };
        let kodiserver2 = Arc::clone(&kodiserver);

        let (koditx, mut kodirx) = channel(100);
        tokio::spawn(async move {
            client::connect(kodiserver2, koditx).await;
        });

        let svr = kodirx.select_next_some().await;
        let (client, kodi_connected) = match svr {
            Event::Connected(client) => (client, true),
            Event::Disconnected(client) => (client, false),
            _ => return Err("Failed to connect to kodi".into()),
        };

        let kodi_status = crate::KodiStatus {
            server: Some(kodiserver),
            ..Default::default()
        };

        Ok(Data {
            kodi_status,
            db: conn,
            kodi_connected,
            client,
            clientrx: kodirx,
        })
    }

    async fn handle_connection(&mut self, mut output: Sender<DataEvent>) -> ! {
        let (sender, mut reciever) = channel(100);
        let item = if self.kodi_connected {
            DataEvent::Online(Connection(sender.clone()), self.client.clone())
        } else {
            DataEvent::Offline(Connection(sender.clone()))
        };
        let _ = output.send(item).await;
        loop {
            select! {
                kodi_msg = self.clientrx.select_next_some() => {
                    match &kodi_msg {
                        Event::Connected(kodi) => {
                            self.client = kodi.clone();
                            self.kodi_connected = true;
                            let _ = output.send(
                                DataEvent::Online(Connection(sender.clone()),
                                kodi.clone())
                            ).await;
                        }
                        Event::Disconnected(kodi) => {
                            self.client = kodi.clone();
                            self.kodi_connected = false;
                            self.kodi_status.active_player_id = None;
                            let _ = output.send(
                                DataEvent::Offline(Connection(sender.clone()))
                            ).await;
                        }
                        _ => {}
                    }

                    let res = self.handle_kodi(&mut output, kodi_msg).await;
                    if res.is_err() {
                        error!("Kodi handler error: {:?}", res.err());
                    }
                }

                msg = reciever.select_next_some() => {
                    debug!(?msg, "Handling UI command");
                    let res = self.handle_cmd(&mut output, msg).await;
                    if res.is_err() {
                        error!("Command handler error: {:?}", res.err());
                    }
                }
            }
        }
    }

    async fn handle_kodi(
        &mut self,
        output: &mut Sender<DataEvent>,
        msg: client::Event,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        match msg {
            Event::UpdatePlayerProps(player_props) => match player_props {
                None => {
                    self.kodi_status.active_player_id = None;
                    self.kodi_status.playing_title = "".into();
                }
                Some(props) => {
                    if self.kodi_status.active_player_id.is_none() {
                        self.client.send(KodiCommand::PlayerGetPlayingItem(
                            props.player_id.expect("player_id should exist"),
                        ));
                    }
                    self.kodi_status.active_player_id = props.player_id;
                    self.kodi_status.player_props = props;
                }
            },
            Event::UpdateKodiAppStatus(status) => {
                self.kodi_status.muted = status.muted;
            }
            Event::UpdatePlayingItem(item) => {
                self.kodi_status.playing_title = item.make_title();
            }
            Event::InputRequested(input) => {
                let _ = output.send(DataEvent::InputRequested(input)).await;
            }

            _ => {}
        }
        // cloning this thing each time seems bad
        // might cchange kodi_status to rwlock
        let _ = output
            .send(DataEvent::KodiStatus(self.kodi_status.clone()))
            .await;
        Ok(())
    }

    // for kodi I must use mpsc instead of oneshot because Clone
    // (even though the command will never actually be cloned)
    async fn handle_cmd(
        &mut self,
        output: &mut Sender<DataEvent>,
        msg: Get,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        match msg {
            Get::Directory { path, media_type } => {
                let (tx, mut rx) = channel(1);
                self.client.send(KodiCommand::GetDirectory {
                    sender: tx,
                    path: path.clone(),
                    media_type,
                });

                let data = rx.select_next_some().await;
                let _ = output.send(DataEvent::ListData { title: path, data }).await;
                Ok(())
            }

            Get::Sources => {
                let (tx, mut rx) = channel(1);
                self.client.send(KodiCommand::GetSources {
                    sender: tx,
                    media_type: MediaType::Video,
                });

                let data = rx.select_next_some().await;
                let _ = output
                    .send(DataEvent::ListData {
                        title: "Sources".into(),
                        data,
                    })
                    .await;
                Ok(())
            }

            Get::KodiServers => {
                let (tx, rx) = oneshot::channel();
                let _ = self.db.send(db::SqlCommand::GetServers { sender: tx });
                let svrs = rx.await?;
                // dbg!(&svrs);
                let _ = output.send(DataEvent::Servers(svrs)).await;
                Ok(())
            }

            Get::AddOrEditServer(srv) => {
                // this command is the only one that's not really "Get"
                // it's mostly just routing front end to db/kodiclient though
                let _ = self.db.send(db::SqlCommand::AddOrEditServer(srv.clone()));
                let _ = self
                    .client
                    .send(KodiCommand::ChangeServer(Arc::new(srv.clone())));
                self.kodi_status.server = Some(Arc::new(srv.clone()));
                // Might change this to send KodiStatus instead.

                // should send out DataEvent::Connected here too?
                let _ = output.send(DataEvent::Servers(vec![srv])).await;
                Ok(())
            }

            Get::Movies => {
                // This sync method is a bit slow with lots of movies
                // I can't really think of a faster way though.
                // sorting the list by dateadded and pulling most 1 or 20 is same speed.

                // --------------
                //  TODO - Consider datestamping my data itself (date retrieved)
                //         Full refresh if stale + date mismatch.
                //         not sure what 'stale' would be..
                // Can also skip check if it was checked very recently maybe?
                // -------------
                self.sync_movies().await;

                let (tx, rx) = oneshot::channel();
                let _ = self.db.send(db::SqlCommand::GetMovieList { sender: tx });
                let data = rx.await?;

                let _ = output
                    .send(DataEvent::ListData {
                        title: "Movies".into(),
                        data,
                    })
                    .await;

                Ok(())
            }

            Get::TVShows => {
                self.sync_tvshows().await;

                let (tx, rx) = oneshot::channel();
                let _ = self.db.send(db::SqlCommand::GetTVShowList { sender: tx });
                let data = rx.await?;

                let _ = output
                    .send(DataEvent::ListData {
                        title: "TV Shows".into(),
                        data,
                    })
                    .await;

                Ok(())
            }

            Get::TVSeasons(tvshowid) => {
                let item = if self.kodi_connected {
                    let (tx, mut rx) = channel(1);
                    let _ = self.client.send(KodiCommand::VideoLibraryGetTVShowDetails {
                        sender: tx,
                        tvshowid,
                    });
                    let show = rx.next().await.expect("Should work if kodi online..");
                    // update the show in db since we loaded it anyway.
                    self.db.send(db::SqlCommand::InsertTVShows {
                        tvshows: vec![show.clone()],
                        do_clean: false,
                    });
                    show
                } else {
                    let (tx, rx) = oneshot::channel();
                    let _ = self.db.send(db::SqlCommand::GetTVShowItem {
                        sender: tx,
                        tvshowid,
                    });
                    rx.await?
                };

                let (tx, rx) = oneshot::channel();
                let _ = self.db.send(db::SqlCommand::GetTVSeasons {
                    sender: tx,
                    tvshowid,
                });
                let mut data = rx.await?;

                if item.season as usize != data.len() && self.kodi_connected {
                    // for seasons pull all and always update all, small data anyway
                    let (tx, mut rx) = channel(1);
                    let _ = self.client.send(KodiCommand::VideoLibraryGetTVSeasons {
                        sender: tx,
                        tvshowid: tvshowid as i32,
                    });
                    let newseasons = rx.select_next_some().await;
                    let _ = self.db.send(db::SqlCommand::InsertTVSeasons(
                        newseasons.clone(),
                        tvshowid,
                    ));

                    data = newseasons.into_iter().map(|v| Box::new(v) as _).collect();
                };

                if !data.is_empty() {
                    let all = TVSeasonListItem {
                        seasonid: 0,
                        tvshowid,
                        season: -1,
                        title: "All Seasons".into(),
                        episode: item.episode,
                    };
                    data.insert(0, Box::new(all));
                }

                let _ = output
                    .send(DataEvent::ListData {
                        title: item.title,
                        data,
                    })
                    .await;

                Ok(())
            }

            Get::TVEpisodes(tvshowid, season) => {
                // due to seasons this one is odd to sync
                // if I ask for last-20 I'm basically repulling the whole last season
                // though I am doing the dateadded check first, so stil a fastpath?
                // seasons complicate it...
                // for now I'm just going to treat the whole show as 1 item to sync.
                self.sync_tvepisodes(tvshowid).await;

                // pull show item from kodi if online and update db?
                // maybe necessary due to 1-season shows skipping season view?
                let (tx, rx) = oneshot::channel();
                let _ = self.db.send(db::SqlCommand::GetTVShowItem {
                    sender: tx,
                    tvshowid,
                });
                let item = rx.await?;

                let title = if season == -1 {
                    format!("{} > All Seasons", item.title)
                } else {
                    format!("{} > Season {}", item.title, season)
                };

                let (tx, rx) = oneshot::channel();
                let _ = self.db.send(db::SqlCommand::GetTVEpisodes {
                    sender: tx,
                    tvshowid,
                    season,
                });

                let data = rx.await?;

                let _ = output.send(DataEvent::ListData { title, data }).await;

                Ok(())
            }
        }
    }

    // might make this also take sample size (instead of hardcoding 20)
    async fn sync_items_by_ids<T, K, B, D, I, G>(
        &mut self,
        get_kodi_ids: K,
        get_batch: B,
        db_delete_ids: D,
        db_insert: I,
        get_db_ids: G,
        batch_size: i32,
    ) where
        T: Clone + Send + 'static,
        K: Fn(Sender<Vec<u32>>) -> KodiCommand,
        B: Fn(Sender<Vec<T>>, Vec<u32>) -> KodiCommand,
        D: Fn(Vec<u32>) -> db::SqlCommand,
        I: Fn(Vec<T>) -> db::SqlCommand,
        G: Fn(oneshot::Sender<Vec<u32>>) -> db::SqlCommand,
    {
        if !self.kodi_connected {
            return;
        }

        // Step 1: Get all IDs from Kodi
        let (kodi_tx, mut kodi_rx) = channel(1);
        self.client.send(get_kodi_ids(kodi_tx));
        let kodi_ids = match kodi_rx.next().await {
            Some(ids) => ids,
            None => return,
        };

        // Step 2: Get all IDs from DB
        let (db_tx, db_rx) = oneshot::channel();
        let _ = self.db.send(get_db_ids(db_tx));
        let db_ids = match db_rx.await {
            Ok(ids) => ids,
            Err(_) => return,
        };

        // Step 3: Compute new, deleted, and existing IDs
        let kodi_ids_set: std::collections::HashSet<u32> = kodi_ids.iter().cloned().collect();
        let db_ids_set: std::collections::HashSet<u32> = db_ids.iter().cloned().collect();

        let new_ids: Vec<u32> = kodi_ids
            .iter()
            .filter(|id| !db_ids_set.contains(id))
            .cloned()
            .collect();
        let deleted_ids: Vec<u32> = db_ids
            .iter()
            .filter(|id| !kodi_ids_set.contains(id))
            .cloned()
            .collect();

        // Step 4: Delete items no longer in Kodi
        if !deleted_ids.is_empty() {
            let _ = self.db.send(db_delete_ids(deleted_ids));
        }

        // Step 5: Fetch new items in batches
        if !new_ids.is_empty() {
            for _batch_ids in new_ids.chunks(batch_size as usize) {
                let (tx, mut rx) = channel(1);
                self.client.send(get_batch(tx, _batch_ids.to_vec()));

                if let Some(items) = rx.next().await {
                    let _ = self.db.send(db_insert(items));
                }
            }
        }
    }

    async fn sync_movies(&mut self) {
        self.sync_items_by_ids(
            |sender| KodiCommand::VideoLibraryGetMovieIDs { sender },
            |sender, ids| KodiCommand::VideoLibraryGetMoviesByIDs { sender, ids },
            |ids| db::SqlCommand::DeleteMoviesByIDs(ids),
            |movies| db::SqlCommand::InsertMovies(movies),
            |sender| db::SqlCommand::GetMovieIDs { sender },
            500, // batch size
        )
        .await;
    }

    async fn sync_tvshows(&mut self) {
        self.sync_items_by_ids(
            |sender| KodiCommand::VideoLibraryGetTVShowIDs { sender },
            |sender, ids| KodiCommand::VideoLibraryGetTVShowsByIDs { sender, ids },
            |ids| db::SqlCommand::DeleteTVShowsByIDs(ids),
            |tvshows| db::SqlCommand::InsertTVShows {
                tvshows,
                do_clean: false, // We handle cleanup via ID deletion now
            },
            |sender| db::SqlCommand::GetTVShowIDs { sender },
            500, // batch size
        )
        .await;
    }

    async fn sync_tvepisodes(&mut self, tvshowid: u32) {
        if !self.kodi_connected {
            return;
        }

        // Get episode IDs from Kodi
        let (kodi_tx, mut kodi_rx) = channel(1);
        self.client.send(KodiCommand::VideoLibraryGetTVEpisodeIDs {
            sender: kodi_tx,
            tvshowid,
        });
        let kodi_episode_ids = match kodi_rx.next().await {
            Some(ids) => ids,
            None => return,
        };

        // Get episode IDs from DB
        let (db_tx, db_rx) = oneshot::channel();
        let _ = self.db.send(db::SqlCommand::GetTVEpisodeIDs {
            sender: db_tx,
            tvshowid,
        });
        let db_episode_ids = match db_rx.await {
            Ok(ids) => ids,
            Err(_) => return,
        };

        // Find new and deleted episodes
        let kodi_ids_set: std::collections::HashSet<u32> =
            kodi_episode_ids.iter().cloned().collect();
        let db_ids_set: std::collections::HashSet<u32> = db_episode_ids.iter().cloned().collect();

        let new_ids: Vec<u32> = kodi_episode_ids
            .iter()
            .filter(|id| !db_ids_set.contains(id))
            .cloned()
            .collect();
        let deleted_ids: Vec<u32> = db_episode_ids
            .iter()
            .filter(|id| !kodi_ids_set.contains(id))
            .cloned()
            .collect();

        // Delete episodes no longer in Kodi
        if !deleted_ids.is_empty() {
            let _ = self.db.send(db::SqlCommand::DeleteTVEpisodesByIDs {
                ids: deleted_ids,
                tvshowid,
            });
        }

        // Fetch new episodes in batches
        let batch_size = 500;
        if !new_ids.is_empty() {
            for _batch_ids in new_ids.chunks(batch_size) {
                let (tx, mut rx) = channel(1);
                // Note: We still need to implement a VideoLibraryGetTVEpisodesByIDs command
                // For now, just fetch them all
                self.client.send(KodiCommand::VideoLibraryGetTVEpisodes {
                    sender: tx,
                    limit: -1,
                    tvshowid,
                });

                if let Some(episodes) = rx.next().await {
                    let _ = self
                        .db
                        .send(db::SqlCommand::InsertTVEpisodes(episodes, tvshowid));
                }
            }
        }
    }
}
