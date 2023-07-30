//use std::sync::mpsc::{Receiver, Sender};

use iced::futures::channel::mpsc::{Receiver, Sender};
use jsonrpsee::core::client::{Client, ClientT, SubscriptionClientT, 
                             Subscription as WsSubscription
                            };
use jsonrpsee::ws_client::WsClientBuilder;
use jsonrpsee::rpc_params;

use serde_json::{Map, Value};
use serde::Deserialize;

// use tokio::time::Duration;

use iced::futures::{self, StreamExt};
use futures::channel::mpsc;
use futures::sink::SinkExt;
use iced::subscription::{self, Subscription};


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
                                    tokio::time::sleep(
                                        tokio::time::Duration::from_secs(1),
                                    ).await;

                                    let _ = output.send(Event::Disconnected).await;
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
async fn handle_connection(
        client: &mut Client, 
        input: &mut Receiver<KodiCommand>, 
        output: &mut Sender<Event>, 
        ) -> Result<(), State> {

    let mut nh: WsSubscription<Map<String, Value>>  = client
        .subscribe_to_method("Player.OnPlay")
        .await.unwrap();
    let mut fnh = nh.by_ref().fuse();

    futures::select! {
        recieved = fnh.select_next_some() => {
            dbg!(recieved.unwrap());
        }


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
                    println!("{} {}", path, mediatype.as_str());
                    
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
                    for source in sources {
                        files.push(crate::ListData{
                            label: source.label,
                            on_click: crate::Message::KodiReq(
                                KodiCommand::GetDirectory{
                                    path: source.file, 
                                    media_type: MediaType::Video,
                                }
                            ),
                        })
                    }

                    
                    let _ = output.send(Event::UpdateFileList { data: files } ).await;

                

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