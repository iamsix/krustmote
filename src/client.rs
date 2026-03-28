use jsonrpsee::core::client::{
    Client, ClientT, Subscription as WsSubscription, SubscriptionClientT,
};
use jsonrpsee::core::params::ObjectParams;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::WsClientBuilder;

use serde::Deserialize;
use serde_json::{Map, Value};

use iced::futures::channel::mpsc::{Receiver, Sender, channel};
use iced::futures::{SinkExt, StreamExt};

use tokio::select;
use tokio::time::{Duration, interval};
use tokio_stream::StreamMap;

use std::error::Error;
use std::sync::Arc;

use crate::koditypes::*;
use tracing::{debug, error};

// TODO: muncher to allow nesting?
macro_rules! rpc_obj_params {
    ($($name:literal=$value:expr),*) => {{
        let mut params = ObjectParams::new();
        $(
            if let Err(err) = params.insert($name, $value) {
                panic!(
                    "Parameter `{}={}` cannot be serialized: {:?}",
                    $name,
                    stringify!($value),
                    err
                );
            }
        )*
        params
    }};
}

#[derive(Debug, Clone)]
pub struct Connection(Sender<KodiCommand>);

impl Connection {
    pub fn send(&mut self, message: KodiCommand) {
        self.0
            .try_send(message)
            .expect("Should be able to send to kodi client");
    }
}

pub async fn connect(svr: Arc<KodiServer>, sender: Sender<Event>) {
    handle_connection(sender, svr).await;
}

async fn handle_connection(mut output: Sender<Event>, mut server: Arc<KodiServer>) -> ! {
    let mut state = State::Disconnected;
    let mut poller = interval(Duration::from_secs(1));
    let mut notifications: StreamMap<&str, WsSubscription<Value>> = StreamMap::new();

    loop {
        match &mut state {
            State::Disconnected => {
                let (ol_sender, ol_reciever) = channel(100);
                let _ = output
                    .send(Event::Disconnected(Connection(ol_sender)))
                    .await;
                match WsClientBuilder::default()
                    .build(server.websocket_url())
                    .await
                {
                    Ok(client) => {
                        let (sender, reciever) = channel(100);
                        let _ = output.send(Event::Connected(Connection(sender))).await;
                        // TODO: More notifications?
                        ws_subscribe(
                            vec!["Player.OnPlay", "Player.OnStop", "Input.OnInputRequested"],
                            &client,
                            &mut notifications,
                        )
                        .await;

                        state = State::Connected(client, reciever);
                    }
                    Err(err) => {
                        error!("Failed to build WS client: {:?}", err);
                        state = State::Offline(ol_reciever);
                    }
                }
            }

            State::Offline(reciever) => {
                // May change this to an auto-backoff eventually.
                match tokio::time::timeout(Duration::from_secs(5), reciever.next()).await {
                    Ok(Some(item)) => match item {
                        KodiCommand::ChangeServer(srv) => {
                            server = srv;
                            state = State::Disconnected;
                        }
                        _ => {}
                    },
                    _ => {
                        // retry connection on timeout
                        // or reciever closed as that will make a new one
                        state = State::Disconnected;
                    }
                }
            }

            State::Connected(client, input) => {
                select! {
                    Some(recieved) = notifications.next() => {
                        debug!(?recieved, "Received WS notification");
                        let (function, data) = recieved;

                        let result = handle_notification(
                            client,
                            function,
                            data
                        ).await;

                        if let Err(err) = result {
                            error!("Notification handler error: {:?}", err);
                            state = State::Disconnected;
                        } else {
                            let _ = output.send(result.unwrap()).await;
                        };

                    }

                    _ = poller.tick() => {
                        match poll_all_status(client).await {
                            Ok(events) => {
                                for event in events {
                                    let _ = output.send(event).await;
                                }
                            }
                            Err(err) => {
                                error!("Polling error: {:?}", err);
                            state = State::Disconnected;
                            }
                        }
                    }





                    message = input.select_next_some() => {
                        debug!(?message, "Processing Kodi command");

                        if let KodiCommand::ChangeServer(srv) = message {
                            server = srv;
                            state = State::Disconnected;
                            // let _ = output.send(Event::Disconnected);
                            continue;
                        };

                        match handle_kodi_command(message, client).await {
                            Ok(event) => {
                                let _ = output.send(event).await;
                            }
                            Err(err) => {
                                error!("Kodi command error: {:?}", err);
                                if is_transport_error(&err) {
                                    state = State::Disconnected;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn ws_subscribe(
    names: Vec<&'static str>,
    client: &Client,
    notifications: &mut StreamMap<&str, WsSubscription<Value>>,
) {
    for name in names {
        let sub: WsSubscription<Value> = client
            .subscribe_to_method(name)
            .await
            .expect("Subscription should always work");
        notifications.insert(name, sub);
    }
}

/// Helper to check if an error should trigger a reconnection
fn is_transport_error(err: &Box<dyn Error + Send + Sync>) -> bool {
    // jsonrpsee errors related to connectivity usually involve the underlying transport
    let s = err.to_string();
    s.contains("Restart needed") || s.contains("Closed") || s.contains("transport")
}

/// Aggregated polling for status
async fn poll_all_status(client: &Client) -> Result<Vec<Event>, Box<dyn Error + Send + Sync>> {
    let mut events = Vec::new();
    events.push(poll_kodi_app_status(client).await?);
    events.push(poll_player_status(client).await?);
    Ok(events)
}

/// Generic helper to request a field and deserialize it
async fn request_field<T>(
    client: &Client,
    method: &str,
    params: ObjectParams,
    field: &str,
) -> Result<T, Box<dyn Error + Send + Sync>>
where
    for<'de> T: Deserialize<'de>,
{
    let response: Value = client.request(method, params).await?;
    Ok(serde_json::from_value(response[field].clone())?)
}

async fn poll_kodi_app_status(client: &Client) -> Result<Event, Box<dyn Error + Send + Sync>> {
    let response: Value = client
        .request(
            "Application.GetProperties",
            rpc_obj_params!("properties" = ["volume", "muted"]),
        )
        .await?;
    let muted: bool = response["muted"]
        .as_bool()
        .expect("`muted: bool` should exist in this response");
    let app_status = KodiAppStatus { muted };
    Ok(Event::UpdateKodiAppStatus(app_status))
}

async fn poll_player_status(client: &Client) -> Result<Event, Box<dyn Error + Send + Sync>> {
    let players: Vec<ActivePlayer> = client
        .request("Player.GetActivePlayers", rpc_params!())
        .await?;

    if players.len() == 0 {
        return Ok(Event::UpdatePlayerProps(None));
    }
    // For now only considering the first playerid it sees...
    let player_id = players[0].playerid;

    let mut playerprops: PlayerProps = client
        .request(
            "Player.GetProperties",
            rpc_obj_params! {"playerid"=player_id, "properties"=PLAYER_PROPS},
        )
        .await?;
    playerprops.player_id = Some(player_id);

    Ok(Event::UpdatePlayerProps(Some(playerprops)))
}

async fn handle_kodi_command(
    message: KodiCommand,
    client: &Client,
) -> Result<Event, Box<dyn Error + Send + Sync>> {
    match message {
        // this was already handled before it got here.
        KodiCommand::ChangeServer(_) => Ok(Event::None), //(Event::Disconnected),

        KodiCommand::GetDirectory {
            mut sender,
            path,
            media_type,
        } => {
            let params = rpc_obj_params!(
                "directory" = path,
                "media" = media_type.as_str(),
                "properties" = FILE_PROPS,
                "sort" = ListSort {
                    method: "date",
                    order: "descending"
                }
            );

            let files: Vec<DirList> =
                request_field(client, "Files.GetDirectory", params, "files").await?;
            let list = files.into_iter().map(|v| Box::new(v) as _).collect();
            let _ = sender.send(list).await;
            Ok(Event::None)
        }

        KodiCommand::GetSources {
            mut sender,
            media_type,
        } => {
            let params = rpc_obj_params!("media" = media_type.as_str());
            let items: Vec<Sources> =
                request_field(client, "Files.GetSources", params, "sources").await?;
            let mut sources: Vec<Box<dyn IntoListData + Send>> =
                items.into_iter().map(|v| Box::new(v) as _).collect();

            let db = Sources {
                label: "- Database".to_string(),
                file: "videoDB://".to_string(),
            };
            sources.insert(0, Box::new(db));

            let _ = sender.send(sources).await;
            Ok(Event::None)
        }

        KodiCommand::PlayerOpen(file) => {
            let params = rpc_obj_params!("item" = serde_json::json!({"file": file}));
            let _: Value = client.request("Player.Open", params).await?;
            Ok(Event::None)
        }

        KodiCommand::InputButtonEvent { button, keymap } => {
            let params = rpc_obj_params!("button" = button, "keymap" = keymap);
            let _: Value = client.request("Input.ButtonEvent", params).await?;
            Ok(Event::None)
        }

        KodiCommand::InputExecuteAction(action) => {
            let _: Value = client
                .request("Input.ExecuteAction", rpc_params![action])
                .await?;
            Ok(Event::None)
        }

        KodiCommand::GUIActivateWindow(window) => {
            let _: Value = client
                .request("GUI.ActivateWindow", rpc_params![window])
                .await?;
            Ok(Event::None)
        }

        KodiCommand::PlayerGetPlayingItem(player_id) => {
            let params = rpc_obj_params!("playerid" = player_id, "properties" = PLAYING_ITEM_PROPS);
            let item: PlayingItem = request_field(client, "Player.GetItem", params, "item").await?;
            Ok(Event::UpdatePlayingItem(item))
        }

        KodiCommand::PlayerSeek(player_id, time) => {
            let params = rpc_obj_params!(
                "playerid" = player_id,
                "value" = serde_json::json!({"time": time})
            );
            let _: Value = client.request("Player.Seek", params).await?;
            Ok(Event::None)
        }

        KodiCommand::PlayerSetSubtitle {
            player_id,
            subtitle_index,
            enabled,
        } => {
            let params = rpc_obj_params!(
                "playerid" = player_id,
                "subtitle" = subtitle_index,
                "enable" = enabled
            );
            let _response: Value = client.request("Player.SetSubtitle", params).await?;
            Ok(Event::None)
        }

        // Kodi RPC is dumb here - see PlayerSetSubtitle for info
        KodiCommand::PlayerToggleSubtitle { player_id, on_off } => {
            let _response: Value = client
                .request(
                    "Player.SetSubtitle",
                    rpc_obj_params!("playerid" = player_id, "subtitle" = on_off),
                )
                .await?;
            Ok(Event::None)
        }

        KodiCommand::PlayerSetAudioStream {
            player_id,
            audio_index,
        } => {
            let _response: Value = client
                .request(
                    "Player.SetAudioStream",
                    rpc_obj_params!("playerid" = player_id, "stream" = audio_index),
                )
                .await?;
            Ok(Event::None)
        }

        KodiCommand::ToggleMute => {
            let _response: Value = client
                .request("Application.SetMute", rpc_obj_params!("mute" = "toggle"))
                .await?;
            // This returns 'false' for muted and 'true' for unmuted but it doesn't matter
            // since we poll for it anyway.

            Ok(Event::None)
        }

        KodiCommand::InputSendText(text) => {
            let _: Value = client.request("Input.SendText", rpc_params!(text)).await?;
            Ok(Event::None)
        }

        KodiCommand::VideoLibraryGetMovies { mut sender, limit } => {
            // I tested tokio::spawn here but kodi itself delays other requests while this runs
            //   (even from other connections/clients/etc)

            let params = if limit != -1 {
                let sort = ListSort {
                    method: "dateadded",
                    order: "descending",
                };
                let limits = ListLimits { end: limit };

                rpc_obj_params!(
                    "properties" = MINIMAL_MOVIE_PROPS,
                    "sort" = sort,
                    "limits" = limits
                )
            } else {
                rpc_obj_params!("properties" = MINIMAL_MOVIE_PROPS)
            };

            let response: Value = client.request("VideoLibrary.GetMovies", params).await?;

            let movies = <Vec<MovieListItem> as Deserialize>::deserialize(&response["movies"])?;

            sender.send(movies).await?;

            Ok(Event::None)
        }

        KodiCommand::VideoLibraryGetTVShows { mut sender, limit } => {
            let params = if limit != -1 {
                let sort = ListSort {
                    method: "dateadded",
                    order: "descending",
                };
                let limits = ListLimits { end: limit };

                rpc_obj_params!(
                    "properties" = MINIMAL_TV_PROPS,
                    "sort" = sort,
                    "limits" = limits
                )
            } else {
                rpc_obj_params!("properties" = MINIMAL_TV_PROPS)
            };
            let response: Value = client.request("VideoLibrary.GetTVShows", params).await?;

            let shows = <Vec<TVShowListItem> as Deserialize>::deserialize(&response["tvshows"])?;

            sender.send(shows).await?;

            Ok(Event::None)
        }

        KodiCommand::VideoLibraryGetTVShowDetails {
            mut sender,
            tvshowid,
        } => {
            // this will fail if the tvshowid is no longer in kodi
            let response: Value = client
                .request(
                    "VideoLibrary.GetTVShowDetails",
                    rpc_obj_params!("tvshowid" = tvshowid, "properties" = MINIMAL_TV_PROPS),
                )
                .await?;

            let show = <TVShowListItem as Deserialize>::deserialize(&response["tvshowdetails"])?;
            sender.send(show).await?;

            Ok(Event::None)
        }

        KodiCommand::VideoLibraryGetTVSeasons {
            mut sender,
            tvshowid,
        } => {
            // tvshowid is an optional req param so I can theoretically
            //   req all seasons then look up the tvshowid in the props
            let response: Value = client
                .request(
                    "VideoLibrary.GetSeasons",
                    rpc_obj_params!("properties" = TV_SEASON_PROPS, "tvshowid" = tvshowid),
                )
                .await?;

            let seasons =
                <Vec<TVSeasonListItem> as Deserialize>::deserialize(&response["seasons"])?;

            sender.send(seasons).await?;

            Ok(Event::None)
        }

        KodiCommand::VideoLibraryGetTVEpisodes {
            mut sender,
            limit,
            tvshowid,
        } => {
            // similar to movies can probably increment with dateadded / limit
            // note tvshowid seems optional but without limit would be huge
            let params = if limit != -1 {
                let sort = ListSort {
                    method: "dateadded",
                    order: "descending",
                };
                let limits = ListLimits { end: limit };

                rpc_obj_params!(
                    "tvshowid" = tvshowid,
                    "properties" = MINIMAL_EP_PROPS,
                    "limits" = limits,
                    "sort" = sort
                )
            } else {
                rpc_obj_params!("tvshowid" = tvshowid, "properties" = MINIMAL_EP_PROPS)
            };

            let response: Value = client.request("VideoLibrary.GetEpisodes", params).await?;

            let episodes =
                <Vec<TVEpisodeListItem> as Deserialize>::deserialize(&response["episodes"])?;

            sender.send(episodes).await?;

            Ok(Event::None)
        }

        // Debug command
        KodiCommand::PlayerGetActivePlayers => {
            let response: Value = client
                .request("Player.GetActivePlayers", rpc_params!())
                .await?;
            debug!(?response, "Active players");
            Ok(Event::None)
        }

        // Debug command
        KodiCommand::PlayerGetProperties => {
            let response: Value = client
                .request(
                    "Player.GetProperties",
                    rpc_obj_params! {"playerid"=1, "properties"=PLAYER_PROPS},
                )
                .await?;
            debug!(?response, "Player properties");
            Ok(Event::None)
        }

        // ID-only fetches for efficient syncing
        KodiCommand::VideoLibraryGetMovieIDs { mut sender } => {
            // Request with empty properties array to get just the IDs (IDs are always returned)
            let response: Value = client
                .request(
                    "VideoLibrary.GetMovies",
                    rpc_obj_params!("properties" = vec![] as Vec<&str>),
                )
                .await?;

            let movies = <Vec<serde_json::Map<String, Value>> as Deserialize>::deserialize(
                &response["movies"],
            )?;
            let ids: Vec<u32> = movies
                .iter()
                .filter_map(|m| {
                    m.get("movieid")
                        .and_then(|v| v.as_u64().map(|id| id as u32))
                })
                .collect();

            sender.send(ids).await?;
            Ok(Event::None)
        }

        KodiCommand::VideoLibraryGetTVShowIDs { mut sender } => {
            // Request with empty properties array to get just the IDs (IDs are always returned)
            let response: Value = client
                .request(
                    "VideoLibrary.GetTVShows",
                    rpc_obj_params!("properties" = vec![] as Vec<&str>),
                )
                .await?;

            let shows = <Vec<serde_json::Map<String, Value>> as Deserialize>::deserialize(
                &response["tvshows"],
            )?;
            let ids: Vec<u32> = shows
                .iter()
                .filter_map(|s| {
                    s.get("tvshowid")
                        .and_then(|v| v.as_u64().map(|id| id as u32))
                })
                .collect();

            sender.send(ids).await?;
            Ok(Event::None)
        }

        KodiCommand::VideoLibraryGetTVEpisodeIDs {
            mut sender,
            tvshowid,
        } => {
            // Request with empty properties array to get just the IDs (IDs are always returned)
            let response: Value = client
                .request(
                    "VideoLibrary.GetEpisodes",
                    rpc_obj_params!("tvshowid" = tvshowid, "properties" = vec![] as Vec<&str>),
                )
                .await?;

            let episodes = <Vec<serde_json::Map<String, Value>> as Deserialize>::deserialize(
                &response["episodes"],
            )?;
            let ids: Vec<u32> = episodes
                .iter()
                .filter_map(|e| {
                    e.get("episodeid")
                        .and_then(|v| v.as_u64().map(|id| id as u32))
                })
                .collect();

            sender.send(ids).await?;
            Ok(Event::None)
        }

        KodiCommand::VideoLibraryGetMoviesByIDs { mut sender, ids } => {
            if ids.is_empty() {
                let _ = sender.send(vec![]).await;
                return Ok(Event::None);
            }

            let mut stream = iced::futures::stream::iter(ids)
                .map(|id| {
                    // let client = client.clone();
                    async move {
                        let params =
                            rpc_obj_params!("movieid" = id, "properties" = MINIMAL_MOVIE_PROPS);
                        request_field::<MovieListItem>(
                            &client,
                            "VideoLibrary.GetMovieDetails",
                            params,
                            "moviedetails",
                        )
                        .await
                    }
                })
                .buffer_unordered(10);

            let mut movies = Vec::new();
            while let Some(res) = stream.next().await {
                if let Ok(movie) = res {
                    movies.push(movie);
                }
            }
            sender.send(movies).await?;
            Ok(Event::None)
        }

        KodiCommand::VideoLibraryGetTVShowsByIDs { mut sender, ids } => {
            if ids.is_empty() {
                let _ = sender.send(vec![]).await;
                return Ok(Event::None);
            }

            let mut stream = iced::futures::stream::iter(ids)
                .map(|id| {
                    //let client = client.clone();
                    async move {
                        let params =
                            rpc_obj_params!("tvshowid" = id, "properties" = MINIMAL_TV_PROPS);
                        request_field::<TVShowListItem>(
                            &client,
                            "VideoLibrary.GetTVShowDetails",
                            params,
                            "tvshowdetails",
                        )
                        .await
                    }
                })
                .buffer_unordered(10);

            let mut shows = Vec::new();
            while let Some(res) = stream.next().await {
                if let Ok(show) = res {
                    shows.push(show);
                }
            }
            sender.send(shows).await?;
            Ok(Event::None)
        }

        KodiCommand::VideoLibraryGetTVEpisodesByIDs { mut sender, ids } => {
            if ids.is_empty() {
                let _ = sender.send(vec![]).await;
                return Ok(Event::None);
            }

            let mut stream = iced::futures::stream::iter(ids)
                .map(|id| {
                    // // let client = client.clone();
                    async move {
                        let params =
                            rpc_obj_params!("episodeid" = id, "properties" = MINIMAL_EP_PROPS);
                        request_field::<TVEpisodeListItem>(
                            &client,
                            "VideoLibrary.GetEpisodeDetails",
                            params,
                            "episodedetails",
                        )
                        .await
                    }
                })
                .buffer_unordered(10);

            let mut episodes = Vec::new();
            while let Some(res) = stream.next().await {
                if let Ok(ep) = res {
                    episodes.push(ep);
                }
            }
            sender.send(episodes).await?;
            Ok(Event::None)
        }

        // debug command
        KodiCommand::PlayerGetPlayingItemDebug(player_id) => {
            let response: Map<String, Value> = client
                .request(
                    "Player.GetItem",
                    rpc_obj_params! {
                        "playerid"=player_id,
                        "properties"=PLAYING_ITEM_PROPS
                    },
                )
                .await?;

            // let playing_item = <PlayingItem as Deserialize>::deserialize(&response["item"])
            //     .expect("PlayingItem should deserialize");
            debug!(?response, "Playing item debug");

            Ok(Event::None)
        }

        // Debug command
        KodiCommand::Test => {
            let response: String = client
                .request("GUI.ShowNotification", rpc_params!["test", "rust"])
                .await?;
            debug!(?response, "Test notification response");
            Ok(Event::None)
        }
    }
    //  None
}

async fn handle_notification(
    client: &Client,
    function: &str,
    data: Result<Value, serde_json::Error>,
) -> Result<Event, Box<dyn Error + Sync + Send>> {
    match function {
        "Player.OnPlay" => {
            let info = data?;
            let player = <ActivePlayer as Deserialize>::deserialize(&info["data"]["player"])?;

            handle_kodi_command(KodiCommand::PlayerGetPlayingItem(player.playerid), client).await
        }

        "Player.OnStop" => {
            let not_playing = PlayingItem::default();
            let not_playing = Event::UpdatePlayingItem(not_playing);
            Ok(not_playing)
        }

        "Input.OnInputRequested" => {
            let info = data?;
            debug!(?info, "Input requested notification");
            let req = info["data"]["value"]
                .as_str()
                .expect("InputReq Notification should contain this value");

            Ok(Event::InputRequested(req.to_string()))
        }

        _ => {
            debug!(function, data = ?data.ok(), "Unhandled notification");
            Ok(Event::None)
        }
    }
}

#[derive(Debug)]
enum State {
    Disconnected,
    Connected(Client, Receiver<KodiCommand>),
    Offline(Receiver<KodiCommand>),
}

#[derive(Debug, Clone)]
pub enum Event {
    Connected(Connection),
    Disconnected(Connection),
    None,
    // UpdateSources(Vec<Sources>),
    // UpdateDirList(Vec<DirList>, String),
    UpdatePlayerProps(Option<PlayerProps>),
    UpdateKodiAppStatus(KodiAppStatus),
    UpdatePlayingItem(PlayingItem), // Might change to Option
    InputRequested(String),
    // UpdateMovieList(Vec<MovieListItem>),
    // UpdateTVList(
    //     Vec<TVShowListItem>,
    //     Vec<TVSeasonListItem>,
    //     Vec<TVEpisodeListItem>,
    // ),
}
