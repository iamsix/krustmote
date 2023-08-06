
use iced::executor;
use iced::theme::Theme;
use iced::font;
// use iced::time;
use iced::widget::{
    button, column, container, row, scrollable, Button, Space, text
};
//use iced::window;
use iced::{
    Color, Application, Command, 
    Element, Length, Settings,  //Subscription,
};

mod icons;
mod client;
//mod recycler;

fn main() -> iced::Result {
   // println!("Hello, world!");
    Rustmote::run(Settings::default())
}

struct Rustmote {
    state: State,
    menu_width: u16,
    file_list: Vec<ListData>,
    kodi_status: KodiStatus,
}

struct KodiStatus {
	now_playing: bool,
 //   active_player: Option<ActivePlayer>,
	muted: bool,
	paused: bool,
	playing_title: String,
    play_time: client::KodiTime,
    duration: client::KodiTime

    // I might keep these as KodiTime for formatting reasons
	// playtime_seconds: u32,
	// duration_seconds: u32,
}

#[derive(Debug, Clone)]
pub struct ListData {
    label: String,
    on_click: Message, 
    play_count: Option<u16>,
    // content_area: Option<String>, // container/element instead?
    bottom_left: Option<String>,
    bottom_right: Option<String>,
    // picture: ???? - not sure if URL or actual image data
}

impl Application for Rustmote {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let kodi_status = KodiStatus{
            now_playing: false,
            muted: false,
            paused: false,
            playing_title: String::from(""),
            play_time: Default::default(),
            duration: Default::default(),

        };
        (
            Self {
                state: State::Disconnected,
                menu_width: 150,
                file_list: Vec::new(),
                kodi_status: kodi_status,
            },
            font::load(include_bytes!("../fonts/MaterialIcons-Regular.ttf").as_slice())
                .map(Message::FontLoaded),
         //   Command::none(),
        )
    }

    fn title(&self) -> String {
        format!("Rustmote - {}", self.kodi_status.playing_title)
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::ToggleLeftMenu => {
                // TODO : Fancy animation by subtracting until 0 etc. maybe.
                self.menu_width = if self.menu_width == 0 {150} else {0};
            }

            Message::ServerStatus(event) => match event {
                
                client::Event::Connected(connection) => {
                    self.state = State::Connected(connection);
                }
                client::Event::Disconnected => {
                    self.state = State::Disconnected;
                }
                client::Event::UpdateFileList { data } => {
                   
                    self.file_list = data;
                }
                client::Event::None => {}
                client::Event::UpdatePlayerProps(player_props) => {
                    match player_props {
                        None => {
                            self.kodi_status.now_playing = false;
                        }
                        Some(props) => {
                            self.kodi_status.now_playing = true;
                            self.kodi_status.paused = props.speed == 0.0;

                            self.kodi_status.play_time = props.time;
                            self.kodi_status.duration = props.totaltime;


                        }
                    }
                }
                client::Event::UpdateKodiAppStatus(status) => {
                    self.kodi_status.muted = status.muted;
                }

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

            _ => {}
        }
        Command::none()
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        client::connect().map(Message::ServerStatus)
    }

    fn view(&self) -> Element<Message> {

        // let time = format!("{hours}:{minutes}:{seconds}", self.kodi_status.play_time);
        // let duration = self.kodi_status.duration_seconds;
        // TODO: properly format the above

        let content = column! [
            // Top Bar thing
            top_bar(self),
            row![
                // Left (menu)
                left_menu(self).explain(Color::from_rgb8(0, 255, 0)),
                //Center (content)
                center_area(self),
                // Right (remote)
                remote(self),
            ].height(Length::Fill),
             // TODO: properly functioning now playing bar
            if self.kodi_status.now_playing { 
                container(
                    row![
                        text(self.kodi_status.playing_title.clone()),

                        if self.kodi_status.paused {
                            icons::pause_clircle_filled().size(24)
                        } else {
                            icons::play_circle_filled().size(24)
                        },
                        text(
                            format!("{} / {}", 
                                self.kodi_status.play_time, 
                                self.kodi_status.duration
                            )
                        )


                    ].spacing(20)
                ).height(50)
            } else {
                container(Space::new(0, 0))
            }
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
    FontLoaded(Result<(), font::Error>),
    ToggleLeftMenu,
    ServerStatus(client::Event),
    KodiReq(client::KodiCommand), // - KodiCommand being an enum likely
}


enum State {
    Disconnected,
    Connected(client::Connection),
}

// TODO : Move these somewhere else / to a different file/struct/etc
fn top_bar<'a>(rustmote: &Rustmote) -> Element<'a, Message> {
    container(
        row![
            button("=").on_press(Message::ToggleLeftMenu),
            Space::new(Length::Fill, Length::Shrink),
            match rustmote.state {
                State::Disconnected => icons::sync_disabled(),
                _ => icons::sync()
            },
        ]
    )
    .into()
}

fn center_area<'a>(rustmote: &'a Rustmote) -> Element<'a, Message> {
    // hopefully thousands of 'buttons' in a list doesn't cause any problems...
    // look in to Lazy and virtual list
    // might be able to fake lazy loading in a weird way using .on_scroll()
    // <spacer ------------><button>...<button><spacer......>
    // not sure if I can easily calculate what items to show though
    scrollable(
        column(
            rustmote.file_list
            .iter()
            .map(make_listitem)
            .map(Element::from)
            .collect()
        )
        .spacing(1)
        .padding(20),
    )
    .width(Length::Fill)
    .into()
}

fn make_listitem(data: &ListData) -> Button<Message> {
    // Let's stretch the definition of a 'button'
    // ___________________________________________________________
    // | picture |  Main Label Information                       |
    // | picture |  (smaller text) content (genre, runtime, etc) |
    // | picture |  bottom left                     bottom right |
    // -----------------------------------------------------------
    //
    // row![ picture, column! [ label, 
    //                          content, 
    //                          row! [bottom_left, space, bottom_right],
    //                         ]
    //     ]
    // It seems pretty clear I'll have to make some kind of custom
    //    RecyclerView type thing.
    //    The button captures any attempt to touch-scroll.
    //    and there's no 'fling' anyway
    Button::new(
        row![                      
            // Temporarily put this here - to be added to Picture later.    
            if data.play_count.unwrap_or(0) > 0 {
                icons::done()
            } else {
                text("")
            },
            column![
                data.label.as_str(),
                row![
                    match &data.bottom_left {
                            Some(d) => {
                                d.as_str()
                            },
                            None => "",
                    },
                    Space::new(Length::Fill, Length::Shrink),
                    match &data.bottom_right {
                        Some(d) => d.as_str(),
                        None => "",
                    },
                ]
            ]
        ],
    ).on_press(
        data.on_click.clone()
    )
    .width(Length::Fill)
}

fn left_menu<'a>(rustmote: &Rustmote) -> Element<'a, Message> {
    container(
        column![
            button(row![icons::folder(), "Files"])
                .on_press(Message::KodiReq(
                    client::KodiCommand::GetSources(
                        client::MediaType::Video)
                    )
                ),
            button("Settings")
        ]
        .spacing(1)
        .padding(10),
    )
    .max_width(rustmote.menu_width)
    .into()
}

fn remote<'a>(rustmote: &Rustmote) -> Element<'a, Message> {
    let red = Color::from_rgb8(255, 0, 0);
    container(
        column![
            // seems like I could template these buttons in some way
            button(icons::bug_report())
                .on_press(Message::KodiReq(client::KodiCommand::Test)),
            button("playerid-test")
                .on_press(Message::KodiReq(client::KodiCommand::PlayerGetActivePlayers)),
            button("props-test")
                .on_press(Message::KodiReq(client::KodiCommand::PlayerGetProperties)),
            row![
                button(icons::volume_down().size(32))
                    .on_press(Message::KodiReq(
                            client::KodiCommand::InputExecuteAction("volumedown")
                    ))
                    .width(40)
                    .height(40),
                if rustmote.kodi_status.muted {
                    button(icons::volume_off().style(red).size(32)).height(40).width(40)
                } else {
                    button(icons::volume_off().size(32)).height(40).width(40)
                },
                button(icons::volume_up().size(32))
                    .on_press(Message::KodiReq(
                            client::KodiCommand::InputExecuteAction("volumeup")
                    ))
                    .width(40)
                    .height(40),

            ].spacing(10),
            row![
                // Might add pgup/pgdn buttons on either side here.
                Space::new(65, 65),    
                button(
                    icons::expand_less().size(48)
                ).width(65)
                .height(65)
                .on_press(Message::KodiReq(
                    client::KodiCommand::InputButtonEvent { 
                        button: "up", 
                        keymap: "R1", 
                    }
                )),
            ].spacing(5),
            row![
                button(
                    icons::chevron_left().size(48)
                ).width(65)
                .height(65)
                .on_press(Message::KodiReq(
                    client::KodiCommand::InputButtonEvent { 
                        button: "left", 
                        keymap: "R1", 
                    }
                )),
                button(
                    icons::circle().size(48)
                ).width(65)
                .height(65)
                .on_press(Message::KodiReq(
                    client::KodiCommand::InputButtonEvent { 
                        button: "select", 
                        keymap: "R1", 
                    }
                )),
                button(
                    icons::chevron_right().size(48)
                ).width(65)
                .height(65)
                .on_press(Message::KodiReq(
                    client::KodiCommand::InputButtonEvent { 
                        button: "right", 
                        keymap: "R1", 
                    }
                )),
            ].spacing(5),
            row![
                button(
                    icons::arrow_back().size(32)
                ).width(65)
                .height(65)
                .on_press(Message::KodiReq(
                    client::KodiCommand::InputButtonEvent { 
                        button: "back", 
                        keymap: "R1", 
                    }
                )),
                button(
                    icons::expand_more().size(48)
                ).width(65)
                .height(65)
                .on_press(Message::KodiReq(
                    client::KodiCommand::InputButtonEvent { 
                        button: "down", 
                        keymap: "R1", 
                    }
                )),
            ].spacing(5),
        ]
        .padding(10)
        .spacing(5),
    )
    .width(220)
    .into()
}
// END TODO
