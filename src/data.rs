// TODO: DATA LAYER
//   datalayer will basically be the 1 subscription that handles all data
// unfortunately leads to lots of boilerplating but should be useful later

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
// Using MPSC channels for this is kind of confusing/complicated though
// data.gettvshows > db.getshows > data.rxdbshows > kodi.getlastdate >
//   data.rxkodidate validated (but how I list my rxdbshows..) > ui.showlist
//   OR
//   data.rxkodidate outdated > kodi.getshows > data.rxkodishows > db.updateshows + db.getshows
//     (now I've looped and need some way to deal with that..)
// I think I need to use oneshots to make this all more linear in a single gettvshows fn

// note it's probably easier to do something like last-10 ordered by dateadded every time
// throw it out if I didn't need it, but if I did I have it already
// if datestamp never matches any of the 10 throw out the entire db and re-pull maybe..
// minorly complicates the logic here but probably better?

use std::sync::Arc;

use crate::client;
use crate::client::Event;
use crate::db;
use crate::koditypes::*;
use iced::futures::channel::mpsc::{channel, Receiver, Sender};
use iced::futures::channel::oneshot;
use iced::futures::{SinkExt, Stream, StreamExt};
use iced::stream;
use std::error::Error;
use tokio::select;

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
    Connected(Connection),
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
    client: client::Connection,
    clientrx: Receiver<client::Event>,
    // tv crumb data?
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
                dbg!(err);
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
        // !!!!!!!!!!!!!!!TODO!!!!!!!!!!!  This needs to deal with empty vec...
        let kodiserver = Arc::new(kodiserver[0].clone());
        let kodiserver2 = Arc::clone(&kodiserver);

        let (koditx, mut kodirx) = channel(100);
        tokio::spawn(async move {
            client::connect(kodiserver2, koditx).await;
        });

        let svr = kodirx.select_next_some().await;
        let client = match svr {
            Event::Connected(client) => client,
            _ => return Err("Failed to connect to kodi".into()),
        };

        let kodi_status = crate::KodiStatus {
            server: Some(kodiserver),
            ..Default::default()
        };

        Ok(Data {
            kodi_status,
            db: conn,
            client,
            clientrx: kodirx,
        })
    }

    async fn handle_connection(&mut self, mut output: Sender<DataEvent>) -> ! {
        let (sender, mut reciever) = channel(100);
        let _ = output
            .send(DataEvent::Online(
                Connection(sender.clone()),
                self.client.clone(),
            ))
            .await;
        loop {
            select! {
                kodi_msg = self.clientrx.select_next_some() => {
                    match &kodi_msg {
                        Event::Connected(kodi) => {
                            self.client = kodi.clone();
                            let _ = output.send(
                                DataEvent::Online(Connection(sender.clone()),
                                kodi.clone())
                            ).await;
                        }
                        Event::Disconnected => {
                            let _ = output.send(
                                DataEvent::Connected(Connection(sender.clone()))
                            ).await;
                        }
                        _ => {}
                    }

                    let res = self.handle_kodi(&mut output, kodi_msg).await;
                    if res.is_err() {
                        dbg!(res.err());
                    }
                }

                msg = reciever.select_next_some() => {
                    dbg!(&msg);
                    let res = self.handle_cmd(&mut output, msg).await;
                    if res.is_err() {
                        dbg!(res.err());
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
                }
                Some(props) => {
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
                let _ = output.send(DataEvent::InputRequested(input));
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
                dbg!(&svrs);
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
                let _ = output.send(DataEvent::Servers(vec![srv])).await;
                Ok(())
            }

            Get::Movies => {
                // --------------
                //  TODO - Consider datestamping my data itself (date retrieved)
                //         Full refresh if stale + date mismatch.
                //         not sure what 'stale' would be..
                // -----------
                // I think I ask db for latest date, ask kodi for latest date
                // compare, then pull resources from there?
                let (tx, rx) = oneshot::channel();
                let _ = self
                    .db
                    .send(db::SqlCommand::GetMostRecentMovieDate { sender: tx });
                let dbdate = rx.await?;

                let (tx, mut rx) = channel(1);
                let _ = self.client.send(KodiCommand::VideoLibraryGetMovies {
                    sender: tx,
                    limit: 20,
                });
                let recentmovies = rx.select_next_some().await;

                if !(recentmovies[0].dateadded == dbdate) {
                    // might change this to let cmd = if ... { command }
                    if recentmovies.iter().any(|mv| mv.dateadded == dbdate) {
                        self.db.send(db::SqlCommand::InsertMovies(recentmovies));
                    } else {
                        // if it's not I probably just want to do a full list refresh
                        todo!("Update the db here - not recent");
                    }
                }

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
                let (tx, rx) = oneshot::channel();
                let _ = self.db.send(db::SqlCommand::GetTVShowItem {
                    sender: tx,
                    tvshowid,
                });
                let item = rx.await?;
                // pull show item from kodi to do the comparison?

                let (tx, rx) = oneshot::channel();
                let _ = self.db.send(db::SqlCommand::GetTVSeasons {
                    sender: tx,
                    tvshowid,
                });
                let mut data = rx.await?;

                // !!!!!!!!!!!!!!!!!!TODO!!!!!!!!!!!!! this check is wrong...
                // I'm checking the db against itself to see if there's no data
                // but I need to check against kodi instead to see if new data.
                // technically the db show will update? (not sure of the conditions when..)
                // This one is kind of odd cause there's almost no need to db it.
                if item.season as usize != data.len() {
                    dbg!("DOING SEASON UPDATE");
                    // for seasons pull all and always update, small data anyway
                    let (tx, mut rx) = channel(1);
                    let _ = self.client.send(KodiCommand::VideoLibraryGetTVSeasons {
                        sender: tx,
                        tvshowid: tvshowid as i32,
                    });
                    let newseasons = rx.select_next_some().await;
                    let _ = self
                        .db
                        .send(db::SqlCommand::InsertTVSeasons(newseasons.clone()));

                    data = newseasons.into_iter().map(|v| Box::new(v) as _).collect();
                };

                let all = TVSeasonListItem {
                    seasonid: 0,
                    tvshowid,
                    season: -1,
                    title: "All Seasons".into(),
                    episode: item.episode,
                };
                data.insert(0, Box::new(all));

                let _ = output
                    .send(DataEvent::ListData {
                        title: item.title,
                        data,
                    })
                    .await;

                Ok(())
            }

            Get::TVEpisodes(tvshowid, season) => {
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
}
