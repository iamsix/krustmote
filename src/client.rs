use jsonrpsee::core::client::{Client, ClientT, SubscriptionClientT, 
                             Subscription as WsSubscription
                            };
use jsonrpsee::ws_client::WsClientBuilder;
use jsonrpsee::rpc_params;

use serde_json::{Map, Value};

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
                        let mut nh: WsSubscription<Map<String, Value>>  = client.subscribe_to_method("Player.OnPlay").await.unwrap();
                        let mut fnh = nh.by_ref().fuse();

                        futures::select! {
                            recieved = fnh.select_next_some() => {
                                println!("recieved: {:?}", recieved);
                            }


                            message = input.select_next_some() => {
                                println!("message: {:?}", message);
                                match message {
                                    KodiCommand::Test => {
                                        let response: Result<String, _> = client.request("GUI.ShowNotification", rpc_params!["test", "rust"]).await;

                                        if response.is_err() {
                                            let _ = output.send(Event::Disconnected).await;
                                            state = State::Disconnected;
                                        } else {
                                            dbg!(response.unwrap());
                                        }
                                    }
                                }

                               // let response: String = client.request("GUI.ShowNotification", rpc_params!["test", "rust"]).await?;
                            }
                        }
                    }
                }
            }
        }
    )

}

#[derive(Debug)]
enum State {
    Disconnected,
    Connected(
        Client,
        mpsc::Receiver<KodiCommand>
    ),
}

#[derive(Debug, Clone)]
pub enum Event {
    Connected(Connection),
    Disconnected,
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
}