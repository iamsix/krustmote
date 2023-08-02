//use std::sync::mpsc::{Receiver, Sender};

use iced::futures::channel::mpsc::{Receiver, Sender};
use jsonrpsee::core::client::{Client, ClientT, SubscriptionClientT, 
                             Subscription as WsSubscription
                            };
use jsonrpsee::ws_client::WsClientBuilder;
use jsonrpsee::rpc_params;

use serde_json::{Map, Value};
use serde::{Serialize, Deserialize};
use serde;

// use tokio::time::Duration;

use iced::futures::{self, StreamExt};
use futures::channel::mpsc;
use futures::sink::SinkExt;
use iced::subscription::{self, Subscription};

//use tracing_subscriber::util::SubscriberInitExt;

const FILE_PROPS: [&str; 20] = [
    "title","rating","genre","artist","track","season","episode","year","duration",
    "album","showtitle","playcount","file","mimetype","size","lastmodified","resume",
    "art","runtime","displayartist"];


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

#[derive(Serialize, Debug)]
struct DirSort {
    method: &'static str,
    order: &'static str,
}

// TODO: I'm sure there's a better way to do this...
// Currently assumes any error is a disconnect
async fn handle_connection(
        client: &mut Client, 
        input: &mut Receiver<KodiCommand>, 
        output: &mut Sender<Event>, 
        ) -> Result<(), State> {

    let mut on_play: WsSubscription<Map<String, Value>>  = client
        .subscribe_to_method("Player.OnPlay")
        .await.unwrap();
    let mut on_play = on_play.by_ref().fuse();

    futures::select! {
        recieved = on_play.select_next_some() => {
            dbg!(recieved.unwrap());
        }

        // TODO: A future that updates the kodi status every 1sec

        message = input.select_next_some() => {
            dbg!(&message);
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
                        let _ = output.send(Event::Disconnected).await;
                        return Err(State::Disconnected);
                    } 
                    let res = response.unwrap();
                    if res != "OK" {
                        dbg!(res);
                    };
                    
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
                        let _ = output.send(Event::Disconnected).await;
                        return Err(State::Disconnected);
                    }

                    let res = response.unwrap();
               //     dbg!(res);
                    let list = <Vec<DirList> as Deserialize>::deserialize(
                            &res["files"]
                        ).unwrap();

                    let mut files: Vec<crate::ListData> = Vec::new();
                    for file in list {

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
                            bottom_right: Some(file.lastmodified),
                            bottom_left: if file.size > 1_073_741_824 {
                                    Some(format!("{:.2} GB", (file.size as f64/1024.0/1024.0/1024.0)))
                            } else {
                                    Some(format!("{:.1} MB", (file.size as f64/1024.0/1024.0)))
                            },
                            
                        })
                    }
                    let _ = output.send(Event::UpdateFileList { data: files } ).await;

                    
                }
                KodiCommand::GetSources(mediatype) => {
                    let response: Result<Map<String, Value>, _> = client.request(
                        "Files.GetSources",
                        rpc_params![mediatype.as_str()],
                    ).await;

                    if response.is_err() {
                        dbg!(response.err());
                        let _ = output.send(Event::Disconnected).await;
                        return Err(State::Disconnected);
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
                            bottom_right: None,
                            bottom_left: None,
                        })
                    }

                    
                    let _ = output.send(Event::UpdateFileList { data: files } ).await;

                

                }
                KodiCommand::PlayerOpen(file) => {

                    #[derive(Serialize)]
                    struct Item{ file : String }
                    let objitem = Item{file: file};
                    let mut params = jsonrpsee::core::params::ObjectParams::new();
                    let _ = params.insert("item", objitem);

                    // {"jsonrpc":"2.0","id":"1","method":"Player.Open","params":{"item":{"file":"Media/Big_Buck_Bunny_1080p.mov"}}}
                    let response: Result<Map<String, Value>, _> = client.request(
                        "Player.Open",
                        params,
                    ).await;

                    if response.is_err() {
                        dbg!(response.err());
                        let _ = output.send(Event::Disconnected).await;
                        return Err(State::Disconnected);
                    }
                    let res = response.unwrap();
                    dbg!(res);
                }
            }
        }
    }
    Ok(())
}




// TODO: proper serde models for all the useful outputs
// Likely need a whole file just to contain them
// Almost need a file of various enums/structs/etc anyway...
#[derive(Deserialize, Debug)]
struct Sources {
    label: String,
    file: String,
}

// TODO: This will need to be much more extensive
//       in order to cover episode 'files' and movie 'files' etc.
//       For now I'm treating everyhing as a directory or file.
#[derive(Deserialize, Debug)]
struct DirList {
    file: String,
    filetype: String,
    label: String,
    lastmodified: String,
    size: u64,
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
    UpdateFileList{data: Vec<crate::ListData>},
//    MessageRecieved(Message),
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
    // InputButtonEvent{button: String, keymap: String},
    // InputExecuteAction(String),
    // ToggleMute,
    // PlayerPlayPause,
    // PlayerStop,
    // GUIActivateWindow(String),

    // Not sure if I actually need these ones from the front end. (they're used by back end)
    // PlayerGetProperties, // Possibly some variant of this one to get subs/audio/video
    // PlayerGetItem,
    // PlayerGetActivePlayers, 
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