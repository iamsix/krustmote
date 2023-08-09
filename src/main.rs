
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
mod koditypes;

use koditypes::*;
//mod recycler;

fn main() -> iced::Result {
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
    muted: bool,
    paused: bool,
    playing_title: String,
    play_time: KodiTime,
    duration: KodiTime
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
                kodi_status,
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
                client::Event::UpdateDirList(dirlist) => {
                    let mut files: Vec<ListData> = Vec::new();
                    for file in dirlist {
                        // dbg!(&file);
                        let label = if file.type_.eq_ignore_ascii_case("episode") {
                            format!(
                                "{} - {}", 
                                file.showtitle.unwrap_or("".to_string()), 
                                file.label
                            )
                        } else {
                            file.label
                        };
        
                        files.push(ListData{
                            label,
                            on_click: Message::KodiReq(
                                match file.filetype.as_str() {
                                    "directory" =>  KodiCommand::GetDirectory{
                                        path: file.file, 
                                        media_type: MediaType::Video,
                                    },
                                    "file" => {
                                        KodiCommand::PlayerOpen(file.file)
                                    },
                                    _ => panic!(
                                        "Impossible kodi filetype {}", 
                                        file.filetype
                                    ),
                                }  
                            ),
                            play_count: file.playcount,
                            bottom_right: Some(file.lastmodified),
                            bottom_left: if file.size > 1_073_741_824 {
                                    Some(format!(
                                        "{:.2} GB", 
                                        (file.size as f64/1024.0/1024.0/1024.0)
                                    ))
                                } else if file.size > 0 {
                                    Some(format!(
                                        "{:.1} MB", 
                                        (file.size as f64/1024.0/1024.0)
                                    ))
                                } else {
                                    None
                                },
                            
                        })
                    }
                    self.file_list = files;

                }
                client::Event::UpdateSources(sources) => {
                    let mut files: Vec<ListData> = Vec::new();
                    files.push(ListData{
                        label: String::from("- Database"), 
                        on_click: Message::KodiReq(
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
                        files.push(ListData{
                            label: source.label,
                            on_click: Message::KodiReq(
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
                    self.file_list = files;
                }
                client::Event::None => {}
                client::Event::UpdatePlayerProps(player_props) => {
                    match player_props {
                        None => {
                            self.kodi_status.now_playing = false;
                        }
                        Some(props) => {
                            // if !self.kodi_status.now_playing
                            //    update playing item info?
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
    KodiReq(KodiCommand),
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
    //
    // TODO: I should specify label heights here to ensure no line wrapping/etc
    Button::new(
        row![                      
            // Watched will proabbly go in picture area - for now just this icon or not    
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
                    KodiCommand::GetSources(
                        MediaType::Video)
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
                .on_press(Message::KodiReq(KodiCommand::Test)),
            button("playerid-test")
                .on_press(Message::KodiReq(KodiCommand::PlayerGetActivePlayers)),
            button("props-test")
                .on_press(Message::KodiReq(KodiCommand::PlayerGetProperties)),
            button("item-test")
                .on_press(Message::KodiReq(KodiCommand::PlayerGetPlayingItem)),
            row![
                button(icons::volume_down().size(32))
                    .on_press(Message::KodiReq(
                        KodiCommand::InputExecuteAction("volumedown")
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
                        KodiCommand::InputExecuteAction("volumeup")
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
                    KodiCommand::InputButtonEvent { 
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
                    KodiCommand::InputButtonEvent { 
                        button: "left", 
                        keymap: "R1", 
                    }
                )),
                button(
                    icons::circle().size(48)
                ).width(65)
                .height(65)
                .on_press(Message::KodiReq(
                    KodiCommand::InputButtonEvent { 
                        button: "select", 
                        keymap: "R1", 
                    }
                )),
                button(
                    icons::chevron_right().size(48)
                ).width(65)
                .height(65)
                .on_press(Message::KodiReq(
                    KodiCommand::InputButtonEvent { 
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
                    KodiCommand::InputButtonEvent { 
                        button: "back", 
                        keymap: "R1", 
                    }
                )),
                button(
                    icons::expand_more().size(48)
                ).width(65)
                .height(65)
                .on_press(Message::KodiReq(
                    KodiCommand::InputButtonEvent { 
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
