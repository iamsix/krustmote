use jsonrpsee::core::client::{
    Client, ClientT, Subscription as WsSubscription, SubscriptionClientT,
};
use jsonrpsee::core::params::ObjectParams;
// use jsonrpsee::core::Error;
use jsonrpsee::core::ClientError;
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::WsClientBuilder;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use iced::futures::channel::mpsc::{channel, Receiver, Sender};
use iced::futures::{SinkExt, StreamExt};
use iced::subscription::{self, Subscription};

use tokio::select;
use tokio::time::{interval, Duration};
use tokio_stream::StreamMap;

use std::error::Error;
use std::sync::Arc;

use crate::koditypes::*;

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

pub fn connect(server: Arc<KodiServer>) -> Subscription<Event> {
    struct Connect;

    subscription::channel(
        std::any::TypeId::of::<Connect>(),
        100,
        |output| async move { handle_connection(output, server).await },
    )
}

async fn handle_connection(mut output: Sender<Event>, mut server: Arc<KodiServer>) -> ! {
    let mut state = State::Disconnected;

    let mut poller = interval(Duration::from_secs(1));
    let mut notifications: StreamMap<&str, WsSubscription<Value>> = StreamMap::new();

    loop {
        match &mut state {
            State::Disconnected => {
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
                        dbg!(err);
                        let _ = output.send(Event::Disconnected).await;
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }

            State::Connected(client, input) => {
                select! {
                    Some(recieved) = notifications.next() => {
                        dbg!(&recieved);
                        let (function, data) = recieved;

                        let result = handle_notification(
                            client,
                            function,
                            data
                        ).await;

                        if result.is_err() {
                            dbg!(result.err());
                            state = State::Disconnected;
                        } else {
                            let _ = output.send(result.unwrap()).await;
                        };

                    }

                    _ = poller.tick() => {
                        // println!("Tick");
                        let app_status =
                            poll_kodi_app_status(client).await;
                        if app_status.is_err() {
                            dbg!(app_status.err());
                            state = State::Disconnected;
                            // if this fails there's no need to poll anything else
                            continue;
                        } else {
                            let _ = output.send(
                                app_status.unwrap()
                            ).await;
                        }

                        let player_props =
                            poll_player_status(client).await;
                        if player_props.is_err() {
                            dbg!(player_props.err());
                            state = State::Disconnected;
                        } else {
                            let _ = output.send(
                                player_props.unwrap()
                            ).await;
                        }
                    }

                    message = input.select_next_some() => {
                        dbg!(&message);
                        if let KodiCommand::ChangeServer(srv) = message {
                            server = srv;
                            state = State::Disconnected;
                            let _ = output.send(Event::Disconnected);
                            continue;
                        };

                        let result = handle_kodi_command(
                            message,
                            client
                        ).await;
                        if result.is_err() {
                            dbg!(result.err());
                            state = State::Disconnected;
                        } else {
                            let _ = output.send(result.unwrap()).await;
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
        KodiCommand::ChangeServer(_) => Ok(Event::Disconnected),

        KodiCommand::GetDirectory { path, media_type } => {
            let response: Map<String, Value> = client
                .request(
                    "Files.GetDirectory",
                    rpc_params![
                        &path,
                        media_type.as_str(),
                        FILE_PROPS,
                        DirSort {
                            method: "date",
                            order: "descending"
                        } // TODO: SortType
                    ],
                )
                .await?;

            let list = <Vec<DirList> as Deserialize>::deserialize(&response["files"])
                .expect("DirList should deserialize");

            Ok(Event::UpdateDirList(list, path))
        }

        KodiCommand::GetSources(mediatype) => {
            let response: Map<String, Value> = client
                .request("Files.GetSources", rpc_params![mediatype.as_str()])
                .await?;

            let mut sources: Vec<Sources> =
                <Vec<Sources> as Deserialize>::deserialize(&response["sources"])
                    .expect("Sources should deserialize");

            let db = Sources {
                label: "- Database".to_string(),
                file: "videoDB://".to_string(),
            };
            sources.insert(0, db);

            Ok(Event::UpdateSources(sources))
        }

        KodiCommand::PlayerOpen(file) => {
            #[derive(Serialize)]
            struct Item {
                file: String,
            }
            let objitem = Item { file };

            let response: String = client
                .request("Player.Open", rpc_obj_params! {"item"=objitem})
                .await?;
            if response != "OK" {
                dbg!(response);
            };

            Ok(Event::None)
        }

        KodiCommand::InputButtonEvent { button, keymap } => {
            let response: String = client
                .request(
                    "Input.ButtonEvent",
                    rpc_obj_params! {"button"=button, "keymap"=keymap},
                )
                .await?;
            if response != "OK" {
                dbg!(response);
            };

            Ok(Event::None)
        }

        KodiCommand::InputExecuteAction(action) => {
            let response: String = client
                .request("Input.ExecuteAction", rpc_params![action])
                .await?;
            if response != "OK" {
                dbg!(response);
            };

            Ok(Event::None)
        }

        KodiCommand::GUIActivateWindow(window) => {
            let response: String = client
                .request("GUI.ActivateWindow", rpc_params![window])
                .await?;
            if response != "OK" {
                dbg!(response);
            };

            Ok(Event::None)
        }

        KodiCommand::PlayerGetPlayingItem(player_id) => {
            let response: Map<String, Value> = client
                .request(
                    "Player.GetItem",
                    rpc_obj_params! {
                        "playerid"=player_id,
                        "properties"=PLAYING_ITEM_PROPS
                    },
                )
                .await?;

            let playing_item = <PlayingItem as Deserialize>::deserialize(&response["item"])
                .expect("PlayingItem should deserialize");

            Ok(Event::UpdatePlayingItem(playing_item))
        }

        KodiCommand::PlayerSeek(player_id, time) => {
            #[derive(Serialize)]
            struct Time {
                time: KodiTime,
            }
            let objtime = Time { time };
            let _response: Value = client
                .request(
                    "Player.Seek",
                    rpc_obj_params!("playerid" = player_id, "value" = objtime),
                )
                .await?;

            // This returns percent/timestamp/duration but we don't really need them
            // because we're scraping every second anyway.
            Ok(Event::None)
        }

        // Kodi RPC kind of ignores the 'enable' field here
        // It disables it for about 10 seconds and re-enables
        // instead you have to set subtitle to "on" or "off" instead of an index ID.
        // So there's a separate PlayerToggleSubtitle for that.
        KodiCommand::PlayerSetSubtitle {
            player_id,
            subtitle_index,
            enabled,
        } => {
            let _response: Value = client
                .request(
                    "Player.SetSubtitle",
                    rpc_obj_params!(
                        "playerid" = player_id,
                        "subtitle" = subtitle_index,
                        "enable" = enabled
                    ),
                )
                .await?;
            dbg!(_response);
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
            dbg!(_response);
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
            dbg!(_response);
            Ok(Event::None)
        }

        KodiCommand::ToggleMute => {
            let _response: Value = client
                .request("Application.SetMute", rpc_obj_params!("mute" = "toggle"))
                .await?;
            // This returns 'false' for muted and 'true' for unmuted but it doesn't matter
            // since we poll for it anyway.
            dbg!(_response);

            Ok(Event::None)
        }

        KodiCommand::InputSendText(text) => {
            let response: Value = client.request("Input.SendText", rpc_params!(text)).await?;
            dbg!(response);
            Ok(Event::None)
        }

        // Testing...
        // TODO: Decide if I want this to be able to directly send data to DB?
        // can do so by cloning the SqlQonnection
        KodiCommand::VideoLibraryGetMovies => {
            // I tested tokio::spawn here but kodi itself delays other requests while this runs
            // (even from other connections/clients/etc)
            let response: Value = client
                .request(
                    "VideoLibrary.GetMovies",
                    rpc_obj_params!("properties" = MINIMAL_MOVIE_PROPS),
                )
                .await?;

            let movies = <Vec<MovieListItem> as Deserialize>::deserialize(&response["movies"])
                .expect("MovieListItem should deserialize");

            Ok(Event::UpdateMovieList(movies))
        }

        // Debug command
        KodiCommand::PlayerGetActivePlayers => {
            let response: Value = client
                .request("Player.GetActivePlayers", rpc_params!())
                .await?;
            dbg!(response);
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
            dbg!(&response);
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
            dbg!(response);

            Ok(Event::None)
        }

        // Debug command
        KodiCommand::Test => {
            let response: String = client
                .request("GUI.ShowNotification", rpc_params!["test", "rust"])
                .await?;
            dbg!(response);
            Ok(Event::None)
        }
    }
    //  None
}

async fn handle_notification(
    client: &Client,
    function: &str,
    data: Result<Value, ClientError>,
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
            dbg!(&info);
            let req = info["data"]["value"]
                .as_str()
                .expect("InputReq Notification should contain this value");

            Ok(Event::InputRequested(req.to_string()))
        }

        _ => {
            dbg!(function, data.unwrap());
            Ok(Event::None)
        }
    }
}

#[derive(Debug)]
enum State {
    Disconnected,
    Connected(Client, Receiver<KodiCommand>),
}

#[derive(Debug, Clone)]
pub enum Event {
    Connected(Connection),
    Disconnected,
    None,
    UpdateSources(Vec<Sources>),
    UpdateDirList(Vec<DirList>, String),
    UpdatePlayerProps(Option<PlayerProps>),
    UpdateKodiAppStatus(KodiAppStatus),
    UpdatePlayingItem(PlayingItem), // Might change to Option
    InputRequested(String),
    UpdateMovieList(Vec<MovieListItem>),
}
