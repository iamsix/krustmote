use jsonrpsee::core::client::{
    Client, ClientT, Subscription as WsSubscription, SubscriptionClientT,
};
use jsonrpsee::core::params::ObjectParams;
use jsonrpsee::core::Error;
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
            .expect("Send command to Kodi server");
    }
}

pub fn connect() -> Subscription<Event> {
    struct Connect;

    subscription::channel(
        std::any::TypeId::of::<Connect>(),
        100,
        |mut output| async move {
            let mut state = State::Disconnected;

            let mut poller = interval(Duration::from_secs(1));
            let mut notifications: StreamMap<&str, WsSubscription<Value>> = StreamMap::new();

            loop {
                match &mut state {
                    State::Disconnected => {
                        const SERVER: &str = "ws://192.168.1.22:9090";
                        match WsClientBuilder::default().build(SERVER).await {
                            Ok(client) => {
                                let (sender, reciever) = channel(100);
                                let _ = output.send(Event::Connected(Connection(sender))).await;

                                // TODO: More notifications?
                                let on_play: WsSubscription<Value> = client
                                    .subscribe_to_method("Player.OnPlay")
                                    .await
                                    .expect("OnPlay Subscription should always work");
                                notifications.insert("OnPlay", on_play);

                                let on_stop: WsSubscription<Value> = client
                                    .subscribe_to_method("Player.OnStop")
                                    .await
                                    .expect("OnStop Subscription should always work");
                                notifications.insert("OnStop", on_stop);

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
                            recieved = notifications.next() => {
                                let (function, data) = recieved
                                    .expect("select should always return data");

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
        },
    )
}

async fn poll_kodi_app_status(client: &mut Client) -> Result<Event, Error> {
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

async fn poll_player_status(client: &mut Client) -> Result<Event, Error> {
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

async fn handle_kodi_command(message: KodiCommand, client: &mut Client) -> Result<Event, Error> {
    match message {
        KodiCommand::GetDirectory {
            path,
            media_type,
        } => {
            let response: Map<String, Value> = client
                .request(
                    "Files.GetDirectory",
                    rpc_params![
                        path,
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

            Ok(Event::UpdateDirList(list))
        }

        KodiCommand::GetSources(mediatype) => {
            let response: Map<String, Value> = client
                .request("Files.GetSources", rpc_params![mediatype.as_str()])
                .await?;

            let sources: Vec<Sources> =
                <Vec<Sources> as Deserialize>::deserialize(&response["sources"])
                    .expect("Sources should deserialize");

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
    client: &mut Client,
    function: &str,
    data: Result<Value, Error>,
) -> Result<Event, Error> {
    match function {
        "OnPlay" => {
            let info = data.expect("select should always return data");

            let player = <ActivePlayer as Deserialize>::deserialize(&info["data"]["player"])
                .expect("OnPlay should contain a player item");

            handle_kodi_command(KodiCommand::PlayerGetPlayingItem(player.playerid), client).await
        }
        "OnStop" => {
            let not_playing = PlayingItem::default();
            let not_playing = Event::UpdatePlayingItem(not_playing);
            Ok(not_playing)
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
    UpdateDirList(Vec<DirList>),
    UpdatePlayerProps(Option<PlayerProps>),
    UpdateKodiAppStatus(KodiAppStatus),
    UpdatePlayingItem(PlayingItem), // Might change to Option
}
