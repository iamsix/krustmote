use iced::executor;
use iced::font;
use iced::theme::{self, Theme};
use iced::widget::scrollable::Id;
// use iced::time;
use iced::widget::{
    button, column, container, image, row, scrollable, text, text_input, Button, Rule, Slider,
    Space,
};

use iced::{subscription, window, Application, Color, Command, Element, Event, Length, Settings};

use ::image as imagelib;
use reqwest;
use tokio::sync::Semaphore;
use urlencoding;

use std::error::Error;
use std::sync::{Arc, OnceLock};
use tokio;

use chrono;

mod client;
mod icons;
mod koditypes;
mod modal;

use modal::Modal;

use koditypes::*;

fn main() -> iced::Result {
    let _ = BLANK_IMAGE.set(image::Handle::from_pixels(80, 120, [0; 38_400]));
    Krustmote::run(Settings::default())
}

struct Krustmote {
    state: State,
    menu_width: u16,
    kodi_status: KodiStatus,
    item_list: ItemList,
    slider_grabbed: bool,
    modal: Modals,
}

struct ItemList {
    data: Vec<ListData>,
    breadcrumb: Vec<KodiCommand>,
    filter: String,
    start_offset: u32,
    visible_count: u32,
}

#[derive(Debug, Clone)]
enum Modals {
    None,
    Subtitles,
    _Video,
    _Audio,
}

const ITEM_HEIGHT: u32 = 55;
static BLANK_IMAGE: OnceLock<image::Handle> = OnceLock::new();

struct KodiStatus {
    now_playing: bool,
    muted: bool,
    paused: bool,
    playing_title: String,
    play_time: KodiTime,
    duration: KodiTime,
}

#[derive(Debug)]
pub struct ListData {
    label: String,
    on_click: Message,
    play_count: Option<u16>,
    // content_area: Option<String>, // container/element instead?
    bottom_left: Option<String>,  // container/element?
    bottom_right: Option<String>, // container/element?
    image: Arc<OnceLock<image::Handle>>,
}

#[derive(Debug, Clone)]
enum Message {
    ToggleLeftMenu,
    UpBreadCrumb,
    ServerStatus(client::Event),
    KodiReq(KodiCommand),
    Scrolled(scrollable::Viewport),
    FilterFileList(String),
    FontLoaded(Result<(), font::Error>),
    WindowResized(u32),
    SliderChanged(u32),
    SliderReleased,
    HideModalAndKodiReq(KodiCommand),
    ShowModal(Modals),
}

enum State {
    Disconnected,
    Connected(client::Connection),
}

impl Application for Krustmote {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let kodi_status = KodiStatus {
            now_playing: false,
            muted: false,
            paused: false,
            playing_title: String::from(""),
            play_time: Default::default(),
            duration: Default::default(),
        };

        let item_list = ItemList {
            data: Vec::new(),
            breadcrumb: Vec::new(),
            start_offset: 0,
            visible_count: 0,
            filter: String::from(""),
        };
        (
            Self {
                state: State::Disconnected,
                menu_width: 150,
                kodi_status,
                item_list,
                slider_grabbed: false,
                modal: Modals::None,
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
                self.menu_width = if self.menu_width == 0 { 150 } else { 0 };
            }

            Message::HideModalAndKodiReq(cmd) => {
                self.modal = Modals::None;
                return Command::perform(async {}, |_| Message::KodiReq(cmd));
            }

            Message::ShowModal(modal) => {
                self.modal = modal;
            }

            Message::WindowResized(height) => {
                // Window height instead of scrollable height is a few extra items
                // but getting the scrollable height is more tedious for little gain.
                self.item_list.visible_count = (height / ITEM_HEIGHT) + 2;
            }

            Message::UpBreadCrumb => {
                let cmd = self.up_breadcrumb();
                return Command::perform(async {}, |_| Message::KodiReq(cmd));
            }

            Message::Scrolled(view) => {
                let offset = (view.absolute_offset().y / ITEM_HEIGHT as f32) as u32;
                self.item_list.start_offset = offset.saturating_sub(1);
            }

            Message::FilterFileList(filter) => {
                self.item_list.filter = filter;
                self.item_list.start_offset = 0;
                return scrollable::snap_to(
                    Id::new("files"),
                    scrollable::RelativeOffset { x: 0.0, y: 0.0 },
                );
            }

            Message::SliderChanged(new) => {
                self.slider_grabbed = true;
                self.kodi_status.play_time.from_seconds(new);
            }

            Message::SliderReleased => {
                self.slider_grabbed = false;
                println!("Slider release: {}", self.kodi_status.play_time);
                let cmd = KodiCommand::PlayerSeek(1, self.kodi_status.play_time.clone());
                return Command::perform(async {}, |_| Message::KodiReq(cmd));
            }

            Message::ServerStatus(event) => match event {
                client::Event::Connected(connection) => {
                    self.state = State::Connected(connection);
                }

                client::Event::Disconnected => {
                    self.state = State::Disconnected;
                }

                client::Event::UpdateDirList(dirlist) => {
                    // TODO = push this to a different fn
                    self.item_list.filter = "".to_string();
                    self.item_list.start_offset = 0;

                    let sem = Arc::new(Semaphore::new(10));

                    let mut files: Vec<ListData> = Vec::new();
                    for file in dirlist {
                        // dbg!(&file);
                        let label = if file.type_ == VideoType::Episode {
                            format!(
                                "{} - S{:02}E{:02} - {}",
                                file.showtitle.unwrap_or("".to_string()),
                                file.season.unwrap_or(0),
                                file.episode.unwrap_or(0),
                                file.title.unwrap_or("".to_string()),
                            )
                        } else {
                            file.label
                        };

                        // Temporary to test image loading
                        let (pic_url, w, h) =
                            if file.type_ == VideoType::Episode && file.art.thumb.is_some() {
                                let thumb = file.art.thumb.unwrap();
                                let thumb = urlencoding::encode(thumb.as_str());
                                (
                                    format!("http://192.168.1.22:8080/image/{}", thumb),
                                    192,
                                    108,
                                )
                            } else if file.art.poster.is_some() {
                                let poster = file.art.poster.unwrap();
                                let poster = urlencoding::encode(poster.as_str());
                                (
                                    format!("http://192.168.1.22:8080/image/{}", poster),
                                    80,
                                    120,
                                )
                            } else {
                                ("".to_string(), 0, 0)
                            };

                        let lock = Arc::new(OnceLock::new());
                        let c_lock = Arc::clone(&lock);
                        // This semaphore limits it to 10 hits on the server at a time.
                        let permit = Arc::clone(&sem).acquire_owned();

                        let pic = if !pic_url.is_empty() {
                            tokio::spawn(async move {
                                let _permit = permit.await;
                                let res = Krustmote::get_pic(pic_url, h, w).await;
                                if let Ok(res) = res {
                                    let _ = c_lock.set(res);
                                } else {
                                    dbg!(res.err());
                                };
                            });
                            lock
                        } else {
                            Arc::new(OnceLock::new())
                        };

                        files.push(ListData {
                            label,
                            on_click: Message::KodiReq(match file.filetype.as_str() {
                                "directory" => KodiCommand::GetDirectory {
                                    path: file.file,
                                    media_type: MediaType::Video,
                                },
                                "file" => KodiCommand::PlayerOpen(file.file),
                                _ => panic!("Impossible kodi filetype {}", file.filetype),
                            }),
                            play_count: file.playcount,
                            bottom_right: Some(file.lastmodified),
                            bottom_left: if file.size > 1_073_741_824 {
                                Some(format!(
                                    "{:.2} GB",
                                    (file.size as f64 / 1024.0 / 1024.0 / 1024.0)
                                ))
                            } else if file.size > 0 {
                                Some(format!("{:.1} MB", (file.size as f64 / 1024.0 / 1024.0)))
                            } else {
                                None
                            },
                            image: pic,
                        })

                        // pic.await;
                    }
                    self.item_list.data = files;

                    //Command::perform(future, f)

                    return scrollable::snap_to(
                        Id::new("files"),
                        scrollable::RelativeOffset { x: 0.0, y: 0.0 },
                    );
                }

                client::Event::UpdateSources(sources) => {
                    // TODO: move this to a different fn
                    self.item_list.filter = "".to_string();
                    self.item_list.start_offset = 0;

                    let mut files: Vec<ListData> = Vec::new();
                    files.push(ListData {
                        label: String::from("- Database"),
                        on_click: Message::KodiReq(KodiCommand::GetDirectory {
                            path: String::from("videoDB://"),
                            media_type: MediaType::Video,
                        }),
                        play_count: None,
                        bottom_right: None,
                        bottom_left: None,
                        image: Arc::new(OnceLock::new()),
                    });
                    for source in sources {
                        files.push(ListData {
                            label: source.label,
                            on_click: Message::KodiReq(KodiCommand::GetDirectory {
                                path: source.file,
                                media_type: MediaType::Video,
                            }),
                            play_count: None,
                            bottom_right: None,
                            bottom_left: None,
                            image: Arc::new(OnceLock::new()),
                        })
                    }
                    self.item_list.data = files;

                    return scrollable::snap_to(
                        Id::new("files"),
                        scrollable::RelativeOffset { x: 0.0, y: 0.0 },
                    );
                }

                client::Event::UpdatePlayerProps(player_props) => match player_props {
                    None => {
                        self.kodi_status.now_playing = false;
                    }
                    Some(props) => {
                        if !self.kodi_status.now_playing {
                            self.kodi_status.now_playing = true;
                            let player_id = props.player_id.expect("player_id should exist");
                            return Command::perform(async {}, move |_| {
                                Message::KodiReq(KodiCommand::PlayerGetPlayingItem(player_id))
                            });
                        }
                        self.kodi_status.now_playing = true;
                        self.kodi_status.paused = props.speed == 0.0;

                        if !self.slider_grabbed {
                            self.kodi_status.play_time = props.time;
                        }
                        self.kodi_status.duration = props.totaltime;
                    }
                },

                client::Event::UpdateKodiAppStatus(status) => {
                    self.kodi_status.muted = status.muted;
                }

                client::Event::UpdatePlayingItem(item) => {
                    if item.type_ == VideoType::Episode {
                        self.kodi_status.playing_title = format!(
                            "{} - S{:02}E{:02} - {}",
                            item.showtitle.unwrap_or("".to_string()),
                            item.season.unwrap_or(0),
                            item.episode.unwrap_or(0),
                            item.title,
                        )
                    } else {
                        self.kodi_status.playing_title = item.label;
                    }
                }

                client::Event::None => {}
            },
            Message::KodiReq(command) => match &mut self.state {
                State::Connected(connection) => {
                    match &command {
                        &KodiCommand::GetSources(_) => {
                            self.item_list.breadcrumb.clear();
                            self.item_list.breadcrumb.push(command.clone());
                        }
                        &KodiCommand::GetDirectory { .. } => {
                            self.item_list.breadcrumb.push(command.clone());
                        }
                        _ => {}
                    }
                    connection.send(command);
                }
                State::Disconnected => {
                    panic!("Kodi is apparently disconnected so I can't");
                }
            },

            _ => {}
        }
        Command::none()
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        iced::Subscription::batch(vec![
            subscription::events_with(|event, _| match event {
                Event::Window(window::Event::Resized { width: _, height }) => {
                    Some(Message::WindowResized(height))
                }
                _ => None,
            }),
            client::connect().map(Message::ServerStatus),
        ])
    }

    fn view(&self) -> Element<Message> {
        let duration = self.kodi_status.duration.total_seconds();
        let play_time = self.kodi_status.play_time.total_seconds();
        let timeleft = duration.saturating_sub(play_time);
        let now = chrono::offset::Local::now();
        let end = now + chrono::Duration::seconds(timeleft as i64);
        let end = end.format("%I:%M %p");
        let content = column![
            // Top Bar thing
            top_bar(self),
            row![
                // Left (menu)
                left_menu(self), //.explain(Color::from_rgb8(0, 255, 0)),
                //Center (content)
                center_area(self),
                // Right (remote)
                remote(self),
            ]
            .height(Length::Fill),
            // TODO: properly functioning now playing bar / move this elswhere.
            if self.kodi_status.now_playing {
                container(
                    row![
                        Space::new(5, 5),
                        column![
                            Slider::new(0..=duration, play_time, Message::SliderChanged)
                                .on_release(Message::SliderReleased),
                            text(format!(
                                "{} / {} ({end})",
                                self.kodi_status.play_time, self.kodi_status.duration
                            )),
                            text(self.kodi_status.playing_title.clone()),
                        ]
                        .width(Length::FillPortion(60)),
                        row![
                            button(if !self.kodi_status.paused {
                                icons::pause_clircle_filled().size(48)
                            } else {
                                icons::play_circle_filled().size(48)
                            })
                            .on_press(Message::KodiReq(
                                KodiCommand::InputExecuteAction("playpause")
                            )),
                            button(icons::stop().size(32)).on_press(Message::KodiReq(
                                KodiCommand::InputExecuteAction("stop")
                            )),
                            button("subtitles").on_press(Message::ShowModal(Modals::Subtitles))
                        ]
                        .width(Length::FillPortion(40))
                        .align_items(iced::Alignment::Center)
                    ]
                    .spacing(20),
                )
                .height(80)
            } else {
                container(Space::new(0, 0))
            }
        ];

        let content: Element<_> = container(content).into();

        match self.modal {
            Modals::Subtitles => {
                // TODO: offload this subtitles dialog elswhere.
                let modal = container(column![
                    row![
                        text("Subtitles").height(40),
                        Space::new(Length::Fill, 10),
                        // This is likely the only place this Message is used
                        // however it's the only way I can think to do this
                        button("Download").on_press(Message::HideModalAndKodiReq(
                            KodiCommand::GUIActivateWindow("subtitlesearch")
                        )),
                    ],
                    Rule::horizontal(5),
                    row![
                        button("-").on_press(Message::KodiReq(KodiCommand::InputExecuteAction(
                            "subtitledelayminus"
                        ))),
                        text(" Delay "),
                        button("+").on_press(Message::KodiReq(KodiCommand::InputExecuteAction(
                            "subtitledelayplus"
                        )))
                    ],
                    // Subtitle adjust buttons.
                ])
                .width(200)
                .padding(10)
                .style(theme::Container::Box); // TODO: style this better.
                Modal::new(content, modal)
                    .on_blur(Message::ShowModal(Modals::None))
                    .into()
            }
            _ => content,
        }

        //  x //.explain(Color::from_rgb8(255, 0, 0))
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
    }
}

impl Krustmote {
    fn up_breadcrumb(&mut self) -> KodiCommand {
        // dbg!(&self.breadcrumb);
        let _ = self.item_list.breadcrumb.pop();
        let command = self.item_list.breadcrumb.pop();
        command.unwrap()
    }

    async fn get_pic(url: String, h: u32, w: u32) -> Result<image::Handle, Box<dyn Error>> {
        // Terrible err handling for now
        //let blank = image::Handle::from_pixels(256, 128, [0]);
        let img = reqwest::get(url).await?;
        let img = img.bytes().await?;

        let img = imagelib::load_from_memory(&img)?;
        let img = img.resize_to_fill(w, h, imagelib::imageops::FilterType::Nearest);
        let img = img.to_rgba8().to_vec();

        Ok(image::Handle::from_pixels(w, h, img))
    }
}

// TODO : Move these somewhere else / to a different file/struct/etc
fn top_bar<'a>(krustmote: &Krustmote) -> Element<'a, Message> {
    container(row![
        button("=").on_press(Message::ToggleLeftMenu),
        Space::new(Length::Fill, Length::Shrink),
        text_input("Filter..", &krustmote.item_list.filter).on_input(Message::FilterFileList),
        match krustmote.state {
            State::Disconnected => icons::sync_disabled(),
            _ => icons::sync(),
        },
    ])
    .into()
}

fn center_area<'a>(krustmote: &'a Krustmote) -> Element<'a, Message> {
    let offset = krustmote.item_list.start_offset;

    let count =
        (offset + krustmote.item_list.visible_count).min(krustmote.item_list.data.len() as u32);

    let mut virtual_list: Vec<Element<'a, Message>> = Vec::new();

    let top_space = offset * ITEM_HEIGHT;
    virtual_list.push(Space::new(10, top_space as f32).into());

    let mut precount: usize = 0;
    let files = krustmote
        .item_list
        .data
        .iter()
        .filter(|&x| {
            x.label
                .to_lowercase()
                .contains(&krustmote.item_list.filter.to_lowercase())
        })
        .enumerate()
        .filter(|&(i, _)| {
            precount = i;
            i as u32 >= offset && i as u32 <= count
        })
        .map(|(_, data)| make_listitem(data))
        .map(Element::from)
        .into_iter();

    virtual_list.extend(files);

    let bottom_space = if !krustmote.item_list.filter.is_empty() {
        precount as u32 * ITEM_HEIGHT
    } else if krustmote.item_list.data.len() > 0 {
        krustmote.item_list.data.len() as u32 * ITEM_HEIGHT
    } else {
        0
    }
    .saturating_sub(offset * ITEM_HEIGHT)
    .saturating_sub(krustmote.item_list.visible_count * ITEM_HEIGHT);

    virtual_list.push(Space::new(10, bottom_space as f32).into());

    // dbg!(virtual_list.len());

    let virtual_list = column(virtual_list);

    column![
        row![if krustmote.item_list.breadcrumb.len() > 1 {
            button("..")
                .on_press(Message::UpBreadCrumb)
                .width(Length::Fill)
                .height(50)
        } else {
            button("").width(Length::Fill).height(50)
        },]
        .spacing(1)
        .padding(5),
        scrollable(virtual_list.spacing(1).padding(5),)
            .on_scroll(Message::Scrolled)
            .id(Id::new("files"))
    ]
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
    let image_data = data.image.get();
    Button::new(row![
        if let Some(img) = image_data {
            container(image(img.clone()).height(45))
        } else {
            // let blank = image::Handle::from_pixels(256, 128, [0; 131072]);
            container(image(BLANK_IMAGE.get().unwrap().clone()).height(45))
        },
        // Watched will proabbly go in picture area - for now just this icon or not
        if data.play_count.unwrap_or(0) > 0 {
            icons::done()
        } else {
            text(" ")
        },
        column![
            text(data.label.as_str()).size(14).height(18),
            text("").size(10),
            row![
                match &data.bottom_left {
                    Some(d) => text(d.as_str()).size(10),
                    None => text(""),
                },
                Space::new(Length::Fill, Length::Shrink),
                match &data.bottom_right {
                    Some(d) => text(d.as_str()).size(10),
                    None => text(""),
                },
            ]
        ]
    ])
    .on_press(data.on_click.clone())
    .width(Length::Fill)
    .height(ITEM_HEIGHT as f32)
}

fn left_menu<'a>(krustmote: &Krustmote) -> Element<'a, Message> {
    container(
        column![
            button(row![icons::folder(), "Files"])
                .on_press(Message::KodiReq(KodiCommand::GetSources(MediaType::Video)))
                .width(Length::Fill),
            button("Settings").width(Length::Fill),
        ]
        .spacing(1)
        .padding(5)
        .width(100),
    )
    .max_width(krustmote.menu_width)
    .into()
}

fn remote<'a>(krustmote: &Krustmote) -> Element<'a, Message> {
    let red = Color::from_rgb8(255, 0, 0);
    container(
        column![
            // seems like I could template these buttons in some way
            button(icons::bug_report()).on_press(Message::KodiReq(KodiCommand::Test)),
            button("playerid-test").on_press(Message::KodiReq(KodiCommand::PlayerGetActivePlayers)),
            button("props-test").on_press(Message::KodiReq(KodiCommand::PlayerGetProperties)),
            button("item-test").on_press(Message::KodiReq(KodiCommand::PlayerGetPlayingItem(1))),
            row![
                button(icons::volume_down().size(32))
                    .on_press(Message::KodiReq(KodiCommand::InputExecuteAction(
                        "volumedown"
                    )))
                    .width(40)
                    .height(40),
                if krustmote.kodi_status.muted {
                    button(icons::volume_off().style(red).size(32))
                        .height(40)
                        .width(40)
                } else {
                    button(icons::volume_off().size(32)).height(40).width(40)
                },
                button(icons::volume_up().size(32))
                    .on_press(Message::KodiReq(KodiCommand::InputExecuteAction(
                        "volumeup"
                    )))
                    .width(40)
                    .height(40),
            ]
            .spacing(10),
            Space::new(Length::Shrink, Length::Fill),
            row![
                // Might add pgup/pgdn buttons on either side here.
                Space::new(65, 65),
                button(icons::expand_less().size(48))
                    .width(65)
                    .height(65)
                    .on_press(Message::KodiReq(KodiCommand::InputButtonEvent {
                        button: "up",
                        keymap: "R1",
                    })),
            ]
            .spacing(5),
            row![
                button(icons::chevron_left().size(48))
                    .width(65)
                    .height(65)
                    .on_press(Message::KodiReq(KodiCommand::InputButtonEvent {
                        button: "left",
                        keymap: "R1",
                    })),
                button(icons::circle().size(48))
                    .width(65)
                    .height(65)
                    .on_press(Message::KodiReq(KodiCommand::InputButtonEvent {
                        button: "select",
                        keymap: "R1",
                    })),
                button(icons::chevron_right().size(48))
                    .width(65)
                    .height(65)
                    .on_press(Message::KodiReq(KodiCommand::InputButtonEvent {
                        button: "right",
                        keymap: "R1",
                    })),
            ]
            .spacing(5),
            row![
                button(icons::arrow_back().size(32))
                    .width(65)
                    .height(65)
                    .on_press(Message::KodiReq(KodiCommand::InputButtonEvent {
                        button: "back",
                        keymap: "R1",
                    })),
                button(icons::expand_more().size(48))
                    .width(65)
                    .height(65)
                    .on_press(Message::KodiReq(KodiCommand::InputButtonEvent {
                        button: "down",
                        keymap: "R1",
                    })),
            ]
            .spacing(5),
        ]
        .padding(10)
        .spacing(5),
    )
    .width(220)
    .into()
}
// END TODO
