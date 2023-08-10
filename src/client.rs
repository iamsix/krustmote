use jsonrpsee::core::client::{Client, ClientT, SubscriptionClientT, 
                             Subscription as WsSubscription};
use jsonrpsee::ws_client::WsClientBuilder;
use jsonrpsee::rpc_params;
use jsonrpsee::core::params::ObjectParams;

use serde_json::{Map, Value};
use serde::{Serialize, Deserialize};

use iced::futures::channel::mpsc::{channel, Receiver, Sender};
use iced::futures::{StreamExt, SinkExt};
use iced::subscription::{self, Subscription};

use tokio::time::{Duration,interval};
use tokio_stream::StreamMap;
use tokio::select;

use crate::koditypes::*;

// TODO: muncher to allow nesting?
macro_rules! rpc_obj_params {
    ($($name:literal=$value:expr),*) => {{
        let mut params = ObjectParams::new();
        $(
            if let Err(err) = params.insert($name, $value) {
                panic!("Parameter `{}={}` cannot be serialized: {:?}", $name, stringify!($value), err);
            }
        )*
        params
    }};
}

const FILE_PROPS: [&'static str; 20] = [
    "title","rating","genre","artist","track","season","episode","year","duration",
    "album","showtitle","playcount","file","mimetype","size","lastmodified","resume",
    "art","runtime","displayartist"];

const PLAYER_PROPS: [&'static str; 17] = [
    "audiostreams","canseek","currentaudiostream","currentsubtitle","partymode",
    "playlistid","position","repeat","shuffled","speed","subtitleenabled","subtitles",
    "time","totaltime","type","videostreams","currentvideostream"];

const PLAYING_ITEM_PROPS: [&'static str; 28]= [
    "album","albumartist","artist","episode","art","file","genre","plot","rating",
    "season","showtitle","studio","tagline","title","track","year","streamdetails",
    "originaltitle","playcount","runtime","duration","cast","writer","director",
    "userrating","firstaired","displayartist","uniqueid"];



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
            let mut notifications: StreamMap<&str, WsSubscription<Value>> = 
                StreamMap::new();

            loop {
                match &mut state {
                    State::Disconnected => {
                        const SERVER: &str = "ws://192.168.1.22:9090";
                        match WsClientBuilder::default().build(SERVER).await{
                                Ok(client) => {
                                    let (sender, reciever) = channel(100);
                                    let _ = output.send(
                                        Event::Connected(Connection(sender))
                                    ).await;

                                    // TODO: More notifications?
                                    let on_play: WsSubscription<Value>  = client
                                        .subscribe_to_method("Player.OnPlay")
                                        .await
                                        .unwrap();
                                    notifications.insert("OnPlay", on_play);

                                    let on_stop: WsSubscription<Value>  = client
                                        .subscribe_to_method("Player.OnStop")
                                        .await
                                        .unwrap();
                                    notifications.insert("OnStop", on_stop);

                                    state = State::Connected(client, reciever);
                                }
                                Err(err) => {
                                    dbg!(err);
                                    let _ = output.send(Event::Disconnected).await;
                                    tokio::time::sleep(
                                        Duration::from_secs(5),
                                    ).await;
                                }
                            }
                        }
                    
                    State::Connected(client, input) => {

                        select! {
                            recieved = notifications.next() => {
                                let (func, data) = recieved.unwrap();
                                // TODO: move this match to a different fn
                                // TBH these notifications are kinda useless
                                //     since I need to poll anyway
                                match func {
                                    "OnPlay" => {
                                        let info = data.unwrap();
                                        dbg!(&info);
                                        let player = <ActivePlayer as Deserialize>::deserialize(
                                            &info["data"]["player"]
                                        ).unwrap();

                                        // possibly look at the item ID and get that if exists?
                                        
                                        let result = handle_kodi_command(
                                            KodiCommand::PlayerGetPlayingItem(player.playerid),
                                            client
                                        ).await;
                                        let result = result.unwrap_or(Event::None);
                                        if matches!(&result, &Event::Disconnected) {
                                            state = State::Disconnected;
                                        } else {
                                            let _ = output.send(result).await;
                                        };

                                    }
                                    "OnStop" => {
                                        let nothing = PlayingItem::default();
                                        let nothing = Event::UpdatePlayingItem(nothing);
                                        let _ = output.send(nothing).await;
                                    }
                                    _ => {
                                        dbg!(func, data.unwrap());
                                    }
                                }
                            }

                            _ = poller.tick() => {
                                // println!("Tick");
                                let app_status = poll_kodi_application_status(client).await;
                                let app_status = app_status.unwrap_or(Event::None);
                                if matches!(&app_status, &Event::Disconnected) {
                                    state = State::Disconnected;
                                    continue;
                                } else {
                                    let _ = output.send(app_status).await;
                                }
                    
                                let player_props = poll_kodi_player_status(client).await;
                                let player_props = player_props.unwrap_or(
                                    Event::UpdatePlayerProps(None)
                                );
                                if matches!(&player_props, &Event::Disconnected) {
                                    state = State::Disconnected;
                                } else {
                                    let _ = output.send(player_props).await;
                                }
                            }
                    
                            message = input.select_next_some() => {
                                dbg!(&message);
                                let result = handle_kodi_command(message, client).await;
                                let result = result.unwrap_or(Event::None);
                                if matches!(&result, &Event::Disconnected) {
                                    state = State::Disconnected;
                                } else {
                                    let _ = output.send(result).await;
                                }
                            }
                        }
                        
                    }
            
                }
            }
        }
    )

}


async fn poll_kodi_application_status (
    client: &mut Client,
) -> Option<Event> {
    let mut params = ObjectParams::new();
    let _ = params.insert("properties", ["volume", "muted"]);

    let response: Result<Value, _> = client.request(
        "Application.GetProperties",
        params,
    ).await;
    if response.is_err() {
        dbg!(response.err());
        return Some(Event::Disconnected);
    }
    let res = response.unwrap();
    let muted: bool = res["muted"].as_bool().unwrap();
    let app_status = KodiAppStatus{muted: muted};
    Some(Event::UpdateKodiAppStatus(app_status))
}


async fn poll_kodi_player_status (
    client: &mut Client,
) -> Option<Event> {
    let response: Result<Value, _> = client.request(
        "Player.GetActivePlayers",
        rpc_params!(),
    ).await;
    if response.is_err() {
        dbg!(response.err());
        return Some(Event::Disconnected);
    }
    let res = response.unwrap();
    let players = <Vec<ActivePlayer> as Deserialize>::deserialize(res).unwrap();
    
    if players.len() == 0 {
        return None;
    }
    // For now only considering the first playerid it sees...
    let player_id = players[0].playerid;

    let response: Result<Value, _> = client.request(
        "Player.GetProperties",
        rpc_obj_params!{"playerid"=player_id, "properties"=PLAYER_PROPS},
    ).await;
    if response.is_err() {
        dbg!(response.err());
        return None;
    } 
    let res = response.unwrap();
    // dbg!(&res);
    let mut playerprops = <PlayerProps as Deserialize>::deserialize(res).unwrap();
    playerprops.player_id = Some(player_id);
    Some(Event::UpdatePlayerProps(Some(playerprops)))
}

async fn handle_kodi_command(
    message: KodiCommand, 
    client: &mut Client
) -> Option<Event> {

    match message {
        // TODO: likely make a generic "OKcommand" structure
        //   that uses the match to determine what "RPC.method", [params]
        //   then just use the same 'request' function and response 
        //   for all of the buttons/etc that just return "OK"
        //
        // I could just fire and forget but I want to handle an error if any.
        KodiCommand::Test => {
            let response: Result<String, _> = client.request(
                "GUI.ShowNotification", 
                rpc_params!["test", "rust"],
            ).await;

            if response.is_err() {
                dbg!(response.err());
                return Some(Event::Disconnected);
            }
            None
            
        }
        KodiCommand::GetDirectory{path, media_type: mediatype} => {
            // Episodes, Shows, Files, Movies.. 
            // probably going to call a separate thing to build the list here
            let response: Result<Map<String, Value>, _> = client.request(
                "Files.GetDirectory",
                rpc_params![
                    path, 
                    mediatype.as_str(), 
                    FILE_PROPS,
                    DirSort{method:"date",order:"descending"} // TODO: SortType
                    ],
                
            ).await;

            if response.is_err() {
                dbg!(response.err());
                return Some(Event::Disconnected);
            }

            let res = response.unwrap();
            // dbg!(&res);
            let list = <Vec<DirList> as Deserialize>::deserialize(
                    &res["files"]
                ).unwrap();

            Some(Event::UpdateDirList(list))
        }

        KodiCommand::GetSources(mediatype) => {
            let response: Result<Map<String, Value>, _> = client.request(
                "Files.GetSources",
                rpc_params![mediatype.as_str()],
            ).await;

            if response.is_err() {
                dbg!(response.err());
                return Some(Event::Disconnected);
            } 

            let res = response.unwrap();
            let sources: Vec<Sources> = 
                <Vec<Sources> as Deserialize>::deserialize(
                    &res["sources"]
                ).unwrap();
            
            Some(Event::UpdateSources(sources))
        }

        KodiCommand::PlayerOpen(file) => {
            #[derive(Serialize)]
            struct Item{ file : String }
            let objitem = Item{file};

            let response: Result<String, _> = client.request(
                "Player.Open",
                rpc_obj_params!{"item"=objitem},
            ).await;

            if response.is_err() {
                dbg!(response.err());
                return Some(Event::Disconnected);
            }
            None
        }

        KodiCommand::InputButtonEvent{button, keymap} => {
            let response: Result<String, _> = client.request(
                "Input.ButtonEvent",
                rpc_obj_params!{"button"=button, "keymap"=keymap},
            ).await;

            if response.is_err() {
                dbg!(response.err());
                return Some(Event::Disconnected);
            }
            None
        }

        KodiCommand::InputExecuteAction(action) => {
            let response: Result<String, _> = client.request(
                "Input.ExecuteAction",
                rpc_params![action],
            ).await;

            if response.is_err() {
                dbg!(response.err());
                return Some(Event::Disconnected);
            }
            None
        }

        KodiCommand::PlayerGetPlayingItem(player_id) => {
            let response: Result<Map<String, Value>, _> = client.request(
                "Player.GetItem",
                rpc_obj_params!{
                    "playerid"=player_id, 
                    "properties"=PLAYING_ITEM_PROPS
                },
            ).await;
            if response.is_err() {
                dbg!(response.err());
                return Some(Event::Disconnected);
            }
            let response = response.unwrap();
            // dbg!(&response);
            let playing_item = <PlayingItem as Deserialize>::deserialize(
                &response["item"]
            ).unwrap();

            Some(Event::UpdatePlayingItem(playing_item))
            
        }
        
        // Debug command
        KodiCommand::PlayerGetActivePlayers => {
            let response: Result<Value, _> = client.request(
                "Player.GetActivePlayers",
                rpc_params!(),
            ).await;
            if response.is_err() {
                dbg!(response.err());
            } else {
                dbg!(response.unwrap());
            }
            None
        }

        // Debug command
        KodiCommand::PlayerGetProperties => {
            let response: Result<Value, _> = client.request(
                "Player.GetProperties",
                rpc_obj_params!{"playerid"=1, "properties"=PLAYER_PROPS},
            ).await;
            if response.is_err() {
                dbg!(response.err());
            } else {
                let res = response.unwrap();
                dbg!(&res);
                let props = <PlayerProps as Deserialize>::deserialize(res).unwrap();
                dbg!(props);
            }
            None
        }
    }
  //  None
}


#[derive(Debug)]
enum State {
    Disconnected,
    Connected(
        Client,
        Receiver<KodiCommand>,
    ),
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
    UpdatePlayingItem(PlayingItem)
}
