
use iced::executor;
use iced::theme::Theme;
// use iced::time;
use iced::widget::{
    button, column, container, row, scrollable
};
//use iced::window;
use iced::{
    Color, Application, Command, 
    Element, Length, Settings,  //Subscription,
};

mod icons;
mod client;


fn main() -> iced::Result {
    println!("Hello, world!");
    Rustmote::run(Settings::default())
}

struct Rustmote {
    state: State,
    menu_width: u16,
    content_data: Vec<ListData>,

}

#[derive(Debug, Default, Clone)]
struct ListData {
    title: String,
    // on_click: Message // ::FillContentArea('whatever') likely?
                         // ::OpenMedia('etc') to send to the back end
}

impl Application for Rustmote {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                state: State::Disconnected,
                menu_width: 150,
                content_data: Vec::new(),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Rustmote - ")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::ToggleLeftMenu => {
                // TODO : Fancy animation by subtracting until 0 etc. maybe.
                self.menu_width = if self.menu_width == 0 {150} else {0};
            }
            Message::FillContentArea(_filldata) => {
                self.content_data.push(ListData{title: String::from("test 1")});
                self.content_data.push(ListData{title: String::from("test 2")});
                
            } 
            Message::ServerStatus(event) => match event {
                client::Event::Connected(connection) => {
                    self.state = State::Connected(connection);
                }
                client::Event::Disconnected => {
                    self.state = State::Disconnected;
                }

                // client::Event::MessageRecieved(msg) => {
                //     println!("msg: {:?}", msg)
                // }
            }
            Message::KodiReq(command) => {
                match &mut self.state {
                    State::Connected(connection) => {
                        connection.send(command);

                    }
                    State::Disconnected => {
                        println!("Kodi is apparently disconnected so I can't");
                    }
                }

            }
        }
        Command::none()
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        client::connect().map(Message::ServerStatus)
    }

    fn view(&self) -> Element<Message> {

        let content = column! [
            // Top Bar thing
            container(
                row![
                    button("=").on_press(Message::ToggleLeftMenu)
                ]
            ),
            row![
                // Left (menu)
                left_menu(self.menu_width).explain(Color::from_rgb8(0, 255, 0)),

                //Center (content)
                scrollable(
                    column(self.content_data
                        .iter()
                        .map(|x| x.title.as_str())
                        .map(Element::from)
                        .collect()
                    )
                    .spacing(20)
                    .padding(20),
                )
                .width(Length::Fill),

                // Right (remote)
                container(
                    column![
                        button("Test")
                            .on_press(Message::KodiReq(client::KodiCommand::Test)),
                        button("^"),
                        row![
                            button("<"),
                            button("O"),
                            button(">"),
                        ],
                        button("v"),
                    ]
                    .padding(10)
                    .spacing(10),
                ).width(100),
            ],

            // Now playing bar goes here.

        ];
        
        let x: Element<_> = container(content).into();

        x //.explain(Color::from_rgb8(255, 0, 0))
        
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
    }



}

#[derive(Debug, Clone)]
enum Message{
    ToggleLeftMenu,
    ServerStatus(client::Event),
  //  ToggleRemote,
  //  ToggleNowPlaying,
    FillContentArea(String), // This will change from String to likely a struct
    KodiReq(client::KodiCommand), // - KodiCommand being an enum likely
}

enum State {
    Disconnected,
    Connected(client::Connection),
}

// TODO : Move these somewhere else / to a different file
fn left_menu<'a>(menu_width: u16) -> Element<'a, Message> {
    container(
        column![
            button(row![icons::folder(), "Files"])
                .on_press(Message::FillContentArea(String::from("Files"))),
            button("Settings")
        ]
        
        .spacing(0)
        .padding(10),
    )
    .max_width(menu_width)
    .into()
}
// END TODO


// Structure the app around the kodi status

// Should probably start by making barebones GUI and hook it up wiwth the client once I have that

// iced::Subscription - useful for exactly what I'm doing.
// https://docs.rs/iced/latest/iced/subscription/index.html
// https://github.com/iced-rs/iced/blob/master/examples/websocket/src/main.rs

/* 
struct Status {
	now_playing: bool,
	kodi: Kodi,
	//theme: Theme

	// Technicallly all the below can go in a different 'container'
	muted: bool,
	paused: bool,
	playing_title: String,
	current_playtime: usize, // Seconds - format for timestamp and use for bar
	duration: usize, // Should be able to do formatting stuff on UI level and use this for bar/etc

}

#[derive(Debug)]
pub enum KodiUpdate {
	Connected,
	Disconnected,
	ConnectionFailed,
	Message, // Not sure on this one but probably?

}
struct Kodi {
	//connection: jsonrpsee_core::client::Client,
	//receiver: mpsc::Receiver<proto::Message>, (this would exist to recieve notifications from server)
}
*/
// async fn connect (server: KodiServer) -> Result<Kodi, connection::Error> {
	
// }

// Likely messages:

// Back-end sends these ones:
// Message::ToggleNowPlaying // (This will update showing playing bar or not)
// Kodi(Update):
//    update::disconnected(probably doesn't need to care if initial connect/disconnect?)
//    update::connected
//    update::connectionfailed

// Front end itself will send these to itself:
// Message::ToggleRemote
// Message::ToggleLeftMenu (left menu bar thing)

/*
Buttons send these ones:
Message::KodiReq('GUI.Shownotification', ['one' 'two'])

Effectively everything stays mostly static except content area
Eventually I make a 'container' for :
StatusBar (top)
Remote (right) (drawer)
PlayingBar (bottom) (hides/shows)
LeftMenu (left) (collapses to small/big)
ContentArea (center)

*/

// 
// Message::ToggleMute (remote)
// Message::PlayPause (playingbar)
// Message::UpdateTitle (playingbar)
// Message::UpdatePlayTime (playingbar)
// Message::UpdateDuration  (playingbar)

// TODO: likely make a separate struct/'container' for the bottom 'playing' bar
// Only the muted, playpause, title, and playtime(/progbar) really need any sort of updating
// every other thing is a button that sends data TO the server
// The back end can hold on to whatever state it needs to and only update UI as needed.

// The one monkey wrench to that is subtitle/audio track/video track selection
// can likely make those separate 'container's
// Additionally the file/movie/etc lists obviously need to pull lots of data from back end

// Need to make a generic message type of like:
//  Message::KodiRqq('GUI.Shownotification', ['one' 'two'])
// KodiReq is likely going to take an enum of some sort
// not sure what Type I'll have to use to define that, since it's either Vec, Map, or None
// Note String by itself is actually not allowed it's just a list of 1 eg. ['volumeup']
// if I'm lucky the rpc_params! macro actually takes care of this for me