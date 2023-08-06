//use std::sync::mpsc::{Receiver, Sender};

use iced::futures::channel::mpsc::{Receiver, Sender};
use jsonrpsee::core::client::{Client, ClientT, SubscriptionClientT, 
                             Subscription as WsSubscription
                            };
use jsonrpsee::ws_client::WsClientBuilder;
use jsonrpsee::rpc_params;
use jsonrpsee::core::params::ObjectParams;

use serde_json::{Map, Value};
use serde::{Serialize, Deserialize};
use serde;


use iced::futures::{self, Future, Stream, StreamExt};
use iced::futures::task::{Poll, Context};
use core::pin::Pin;
use futures::channel::mpsc;
use futures::sink::SinkExt;
use iced::subscription::{self, Subscription};

use tokio::time::{self, Duration, Instant};


const FILE_PROPS: [&str; 20] = [
    "title","rating","genre","artist","track","season","episode","year","duration",
    "album","showtitle","playcount","file","mimetype","size","lastmodified","resume",
    "art","runtime","displayartist"];

const PLAYER_PROPS: [&str; 17] = [
    "audiostreams","canseek","currentaudiostream","currentsubtitle","partymode",
    "playlistid","position","repeat","shuffled","speed","subtitleenabled","subtitles",
    "time","totaltime","type","videostreams","currentvideostream"];





pub fn connect() -> Subscription<Event> {
    struct Connect;

    subscription::channel(
        std::any::TypeId::of::<Connect>(),
        100,
        |mut output| async move {
            let mut state = State::Disconnected;

            loop {
                match &mut state {
                    State::Disconnected => {
                        const SERVER: &str = "ws://192.168.1.22:9090";

                        match WsClientBuilder::default()
                            .build(SERVER).await
                            {
                                Ok(client) => {
                                    let (sender, reciever) = mpsc::channel(100);

                                    let _ = output.send(
                                        Event::Connected(Connection(sender))
                                    ).await;

                                    state = State::Connected(client, reciever);

                                }
                                Err(_) => {
                                    let _ = output.send(Event::Disconnected).await;
                                    tokio::time::sleep(
                                        tokio::time::Duration::from_secs(5),
                                    ).await;
                                }
                            }
                        }
                    
                    State::Connected(client, input) => {
                        let res = handle_connection(client, input, &mut output).await;
                        if res.is_err() {
                            state = res.unwrap_err();
                        }
                        
                    }
             
                }
            }
        }
    )

}


// TODO: I'm sure there's a better way to do this...
// Currently assumes any error is a disconnect.
// It SHOULD look at the actual error since it could return others (ie parse errors etc)

// This should probably return an Option instead?
async fn handle_connection(
        client: &mut Client, 
        input: &mut Receiver<KodiCommand>, 
        output: &mut Sender<Event>, 
        ) -> Result<(), State> {

    let mut on_play: WsSubscription<Map<String, Value>>  = client
        .subscribe_to_method("Player.OnPlay")
        .await
        .unwrap();
    let mut on_play = on_play.by_ref().fuse();

   let poller = Every::new(1);
   let mut poller = poller.fuse();

    futures::select! {
        recieved = on_play.select_next_some() => {
            // Player.GetItem
            dbg!(recieved.unwrap());
        }

        _ = poller.select_next_some() => {
            let app_status = poll_kodi_application_status(client).await;
            let app_status = app_status.unwrap_or(Event::None);
            let _ = output.send(app_status).await;

            let player_props = poll_kodi_player_status(client).await;
            let player_props = player_props.unwrap_or(Event::UpdatePlayerProps(None));
            if matches!(&player_props, &Event::Disconnected) {
                return Err(State::Disconnected);
            }
            let _ = output.send(player_props).await;
        }

        message = input.select_next_some() => {
            dbg!(&message);
            let result = handle_kodi_command(message, client).await;
            let result = result.unwrap_or(Event::None);
            if matches!(&result, &Event::Disconnected) {
                return Err(State::Disconnected);
            }
            let _ = output.send(result).await;
            
        }
    }
    Ok(())
}


async fn poll_kodi_application_status (
    client: &mut Client,
//    output: &mut Sender<Event>,
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
//    output: &mut Sender<Event>,
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

    // For now we only consider player[0]
    let player_id = players[0].playerid;
    let mut params = ObjectParams::new();
    let _ = params.insert("playerid", player_id);
    let _ = params.insert("properties", PLAYER_PROPS);

    let response: Result<Value, _> = client.request(
        "Player.GetProperties",
        params,
    ).await;
    if response.is_err() {
        dbg!(response.err());
        return None;
    } 
    let res = response.unwrap();
    let playerprops = <PlayerProps as Deserialize>::deserialize(res).unwrap();
    Some(Event::UpdatePlayerProps(Some(playerprops)))
}

// // This seems like a hack but is also the 'cleanest' way I could think to do this
struct Every {
    sleep: Pin<Box<time::Sleep>>,
    delay: u64,
}

impl Every {
    fn new(seconds: u64) -> Self {
        Self {
            delay: seconds,
            sleep: Box::pin(time::sleep(Duration::from_secs(seconds)))
        }
    }
}

impl Future for Every {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.sleep.as_mut().poll(cx)
    }
}

impl Stream for Every {
    // Type doesn't actually matter, just had to be something
    type Item = ();
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>
    ) -> Poll<Option<()>> {
        match self.sleep.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => {
                let delay = self.delay;
                self.sleep.as_mut().reset(
                    Instant::now() + Duration::from_secs(delay)
                );
                Poll::Ready(Some(()))
            }
        }
    }
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
        // there are some special ones that actually have output
        KodiCommand::Test => {
            let response: Result<String, _> = client.request(
                "GUI.ShowNotification", 
                rpc_params!["test", "rust"],
            ).await;

            if response.is_err() {
               //  let _ = output.send(Event::Disconnected).await;
                return Some(Event::Disconnected);
            } 
            let res = response.unwrap();
            if res != "OK" {
                dbg!(res);
            };
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

            let mut files: Vec<crate::ListData> = Vec::new();
            for file in list {
                // dbg!(&file);

                files.push(crate::ListData{
                    label: file.label,
                    on_click: crate::Message::KodiReq(
                        match file.filetype.as_str() {
                            "directory" =>  KodiCommand::GetDirectory{
                                path: file.file, 
                                media_type: MediaType::Video,
                            },
                            "file" => {
                                KodiCommand::PlayerOpen(file.file)
                            },
                            _ => panic!("Impossible kodi filetype {}", file.filetype),
                        }  
                    ),
                    play_count: file.playcount,
                    bottom_right: Some(file.lastmodified),
                    /// Should the front end be doing this?
                    /// if it does though it would need to know WHAT conent it is
                    bottom_left: if file.size > 1_073_741_824 {
                            Some(format!("{:.2} GB", (file.size as f64/1024.0/1024.0/1024.0)))
                        } else if file.size > 0 {
                            Some(format!("{:.1} MB", (file.size as f64/1024.0/1024.0)))
                        } else {
                            None
                        },
                    
                })
            }
            return Some(Event::UpdateFileList { data: files });

            
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
            
            // TODO: custom deserialize directly in to ListData? undecided.
            // Might deserialize to vec<struct>
            //  and let the front end figure out the rest.
            // This is more important on GetDirectory
            //  and especially once I do the movie/tv data back-end.
            let mut files: Vec<crate::ListData> = Vec::new();
            files.push(crate::ListData{
                label: String::from("- Database"), 
                on_click: crate::Message::KodiReq(
                    KodiCommand::GetDirectory{
                        path: String::from("videoDB://"),
                        media_type: MediaType::Video,
                    }
                ),
                play_count: None,
                bottom_right: None,
                bottom_left: None,
                
            });
            for source in sources {
                files.push(crate::ListData{
                    label: source.label,
                    on_click: crate::Message::KodiReq(
                        KodiCommand::GetDirectory{
                            path: source.file, 
                            media_type: MediaType::Video,
                        }
                    ),
                    play_count: None,
                    bottom_right: None,
                    bottom_left: None,
                })
            }
            Some(Event::UpdateFileList { data: files } )

        }

        // Probably OK Command
        KodiCommand::PlayerOpen(file) => {

            #[derive(Serialize)]
            struct Item{ file : String }
            let objitem = Item{file: file};
            let mut params = ObjectParams::new();
            let _ = params.insert("item", objitem);

            // {"jsonrpc":"2.0","id":"1","method":"Player.Open","params":{"item":{"file":"Media/Big_Buck_Bunny_1080p.mov"}}}
            let response: Result<String, _> = client.request(
                "Player.Open",
                params,
            ).await;

            if response.is_err() {
                dbg!(response.err());
                return Some(Event::Disconnected);
            }
            let res = response.unwrap();
            dbg!(res);
            None
        }

        // Probably OK Command
        KodiCommand::InputButtonEvent{button, keymap} => {
            let mut params = ObjectParams::new();
            let _ = params.insert("button", button);
            let _ = params.insert("keymap", keymap);
            let response: Result<String, _> = client.request(
                "Input.ButtonEvent",
                params,
            ).await;

            if response.is_err() {
                dbg!(response.err());
                return Some(Event::Disconnected);
            }
            let res = response.unwrap();
            dbg!(res);
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
            let res = response.unwrap();
            dbg!(res);
            None
        }

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

        KodiCommand::PlayerGetProperties => {
            let mut params = ObjectParams::new();
            let _ = params.insert("playerid", 1);
            let _ = params.insert("properties", PLAYER_PROPS);

            let response: Result<Map<String, Value>, _> = client.request(
                "Player.GetProperties",
                params,
            ).await;
            if response.is_err() {
                dbg!(response.err());
            } else {
                dbg!(response.unwrap());
            }
            None
        }
    }
  //  None
}


#[derive(Deserialize, Clone, Debug)]
pub struct PlayerProps {
    pub speed: f64,
    pub time: KodiTime,
    pub totaltime: KodiTime,
    // currentaudiostream: AudioStream,
    // audiostreams: Vec[AudioStream],
    // canseek: bool,
    // currentsubtitle: Subtitle,
    // subtitles: Vec[Subtitles]
    // currentvideostream: VideoStream,
    // videostreams: Vec[VideoStream],
    // playlistid: u8,
    // position: u8,
    // repeat: String //(could be enum?)
    // shuffled: bool,
    // subtitleenabled: bool,
    // type_: MediaType // need impl fromstring

}


#[derive(Deserialize, Clone, Debug, Default)]
pub struct KodiTime {
    pub hours: u8,
    pub milliseconds: i16,
    pub minutes: u8,
    pub seconds: u8,
}

impl std::fmt::Display for KodiTime {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:02}:{:02}:{:02}", self.hours, self.minutes, self.seconds)
    }
}



#[derive(Deserialize, Debug)]
struct ActivePlayer {
    playerid: u8,
  //  playertype: String,
  //  type_: MediaType //need to impl 'from' string on that.
}


// TODO: proper serde models for all the useful outputs
// Likely need a whole file just to contain them
// Almost need a file of various enums/structs/etc anyway...
#[derive(Deserialize, Debug)]
struct Sources {
    label: String,
    file: String,
}


#[derive(Serialize, Debug)]
struct DirSort {
    method: &'static str,
    order: &'static str,
}

// TODO: This will need to be much more extensive
//       in order to cover episode 'files' and movie 'files' etc.
//       For now I'm treating everyhing as a generic directory or file.
#[derive(Deserialize, Debug)]
struct DirList {
    file: String,
    filetype: String,
    label: String,
    lastmodified: String,
    size: u64,
    playcount: Option<u16>,
}


#[derive(Debug)]
enum State {
    Disconnected,
    Connected(
        Client,
        mpsc::Receiver<KodiCommand>,
    ),
}

#[derive(Debug, Clone)]
pub enum Event {
    Connected(Connection),
    Disconnected,
    None,
    UpdateFileList{data: Vec<crate::ListData>},
    UpdatePlayerProps(Option<PlayerProps>),
    UpdateKodiAppStatus(KodiAppStatus)
}

#[derive(Deserialize, Debug, Clone)]
pub struct KodiAppStatus {
    pub muted: bool,
    //volume: u8,
}

#[derive(Debug, Clone)]
pub struct Connection(mpsc::Sender<KodiCommand>);

impl Connection {
    pub fn send(&mut self, message: KodiCommand) {
        self.0
            .try_send(message)
            .expect("Send command to Kodi server");
    }
}

#[derive(Debug, Clone)]
pub enum KodiCommand {
    Test,
    GetSources(MediaType), // TODO: SortType
    GetDirectory{path: String, media_type: MediaType}, // TODO: SortType
    PlayerOpen(String),
    InputButtonEvent{button: &'static str, keymap: &'static str},
    InputExecuteAction(&'static str),
    // ToggleMute,
    // PlayerPlayPause,
    // PlayerStop,
    // GUIActivateWindow(String),

    // Not sure if I actually need these ones from the front end. (they're used by back end)
     PlayerGetProperties, // Possibly some variant of this one to get subs/audio/video
    // PlayerGetItem,
     PlayerGetActivePlayers, 
}

#[derive(Debug, Clone, Copy)]
pub enum MediaType {
    Video,
   // Music,
   // Pictures,
   // Files,
   // Programs,
}

impl MediaType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::Video => "video",
        }

    }
}