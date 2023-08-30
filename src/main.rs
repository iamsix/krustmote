use iced::executor;
use iced::font;
use iced::theme::Theme;
use iced::widget::scrollable::Id;
// use iced::time;
use iced::widget::{column, container, image, row, scrollable};

use iced::{subscription, window, Application, Command, Element, Event, Length, Settings};

use ::image as imagelib;
use reqwest;
use tokio::sync::Semaphore;
use urlencoding;

use std::error::Error;
use std::sync::{Arc, OnceLock};
use tokio;

mod client;
mod db;
mod icons;
mod koditypes;
mod modal;
mod settingsui;
mod themes;
mod uiparts;

use modal::Modal;

use koditypes::*;

fn main() -> iced::Result {
    // TODO: Move this somewhere else.
    let img = imagelib::load_from_memory_with_format(
        include_bytes!("../icon.png"),
        imagelib::ImageFormat::Png,
    );

    let window = match img {
        Ok(img) => {
            let icon = img.as_rgba8().unwrap();
            window::Settings {
                icon: window::icon::from_rgba(icon.to_vec(), icon.width(), icon.height()).ok(),
                ..Default::default()
            }
        }
        Err(_) => window::Settings {
            ..Default::default()
        },
    };

    let _ = BLANK_IMAGE.set(image::Handle::from_pixels(80, 120, [0; 38_400]));
    Krustmote::run(Settings {
        window,
        ..Settings::default()
    })
}

struct Krustmote {
    state: State,
    db_state: DbState,
    menu_width: u16,
    kodi_status: KodiStatus,
    item_list: ItemList,
    slider_grabbed: bool,
    send_text: String,
    content_area: ContentArea,
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
    RequestText,
    _Video,
    Audio,
}

enum ContentArea {
    Files,
    Loading,
    Settings(settingsui::Settings),
    _ItemInfo,
}

const ITEM_HEIGHT: u32 = 55;
static BLANK_IMAGE: OnceLock<image::Handle> = OnceLock::new();

// TODO: consider directly using PlayerProps and PlayingItem
//       this basically just re-makes those structs anyway...
struct KodiStatus {
    server: Option<Arc<KodiServer>>,
    active_player_id: Option<u8>,
    muted: bool,
    playing_title: String,
    // playing_item: PlayingItem,
    player_props: PlayerProps,
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
    DbEvent(db::Event),
    DbQuery(db::SqlCommand),
    Settings(settingsui::Message),
    SettingsEvent(settingsui::Event),
    Scrolled(scrollable::Viewport),
    FilterFileList(String),
    FontLoaded(Result<(), font::Error>),
    WindowResized(u32),
    SliderChanged(u32),
    SliderReleased,
    HideModalAndKodiReq(KodiCommand),
    ShowModal(Modals),
    SubtitlePicked(Subtitle),
    SubtitleToggle(bool),
    AudioStreamPicked(AudioStream),
    SendTextInput(String),
}

enum State {
    Disconnected,
    Connected(client::Connection),
}

enum DbState {
    Closed,
    Open(db::SqlConnection),
}

impl Application for Krustmote {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let kodi_status = KodiStatus {
            server: None,
            active_player_id: None,
            muted: false,
            playing_title: "".to_string(),
            player_props: Default::default(),
            // playing_item: Default::default(),
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
                db_state: DbState::Closed,
                menu_width: 150,
                kodi_status,
                item_list,
                slider_grabbed: false,
                send_text: String::from(""),
                content_area: ContentArea::Files,
                modal: Modals::None,
            },
            font::load(include_bytes!("../fonts/MaterialIcons-Regular.ttf").as_slice())
                .map(Message::FontLoaded),
            //   Command::none(),
        )
    }

    fn title(&self) -> String {
        format!("Krustmote - {}", self.kodi_status.playing_title)
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::Settings(settings_msg) => {
                if let ContentArea::Settings(set) = &mut self.content_area {
                    return set.update(settings_msg).map(Message::SettingsEvent);
                }
            }

            Message::SettingsEvent(event) => match event {
                settingsui::Event::AddServer(srv) => {
                    let q = db::SqlCommand::AddOrEditServer(srv);
                    return Command::perform(async {}, |_| Message::DbQuery(q));
                },
            },

            Message::ToggleLeftMenu => {
                // TODO : Fancy animation by subtracting until 0 etc. maybe.
                self.menu_width = if self.menu_width == 0 { 150 } else { 0 };
            }

            Message::HideModalAndKodiReq(cmd) => {
                self.modal = Modals::None;

                if matches!(cmd, KodiCommand::InputSendText(_)) {
                    self.send_text = "".to_string();
                }

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

            Message::SubtitlePicked(sub) => {
                let cmd = KodiCommand::PlayerSetSubtitle {
                    player_id: self
                        .kodi_status
                        .active_player_id
                        .expect("Should be playing if this is called"),
                    subtitle_index: sub.index,
                    enabled: self.kodi_status.player_props.subtitleenabled,
                };
                return Command::perform(async {}, |_| Message::KodiReq(cmd));
            }

            Message::SubtitleToggle(val) => {
                self.kodi_status.player_props.subtitleenabled = val;
                let on_off = if val { "on" } else { "off" };
                let cmd = KodiCommand::PlayerToggleSubtitle {
                    player_id: self
                        .kodi_status
                        .active_player_id
                        .expect("Should be playing if this is called"),
                    on_off,
                };
                return Command::perform(async {}, |_| Message::KodiReq(cmd));
                // send SubtitlePicked here with current_subtitle?
            }

            Message::SendTextInput(text) => {
                self.send_text = text;
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
                self.kodi_status.player_props.time.set_from_seconds(new);
            }

            Message::SliderReleased => {
                self.slider_grabbed = false;
                // println!("Slider release: {}", self.kodi_status.play_time);
                let cmd = KodiCommand::PlayerSeek(
                    self.kodi_status
                        .active_player_id
                        .expect("should have a player_id if this is visible"),
                    self.kodi_status.player_props.time.clone(),
                );
                return Command::perform(async {}, |_| Message::KodiReq(cmd));
            }

            Message::DbEvent(event) => {
                match event {
                    db::Event::Closed => {}

                    db::Event::Opened(conn) => {
                        self.db_state = DbState::Open(conn);

                        // upon open we read config and servers
                        return Command::perform(async {}, |_| {
                            Message::DbQuery(db::SqlCommand::GetServers)
                        });
                    }

                    db::Event::UpdateServers(servers) => {
                        dbg!(&servers);
                        if servers.len() == 0 {
                            let new_server = settingsui::Settings::new();
                            self.content_area = ContentArea::Settings(new_server);
                        } else {
                            // We currently only care about 1 server until we
                            // have the settings table to get the selected server
                            self.kodi_status.server = Some(Arc::new(servers[0].clone()));
                            self.content_area = ContentArea::Files;
                        }
                    }

                    db::Event::None => {}
                }
            }

            Message::DbQuery(command) => match &mut self.db_state {
                DbState::Closed => {
                    panic!("DB not opened?")
                }

                DbState::Open(conn) => {
                    conn.send(command);
                }
            },

            Message::ServerStatus(event) => {
                if let Some(value) = self.handle_server_event(event) {
                    return value;
                }
            }

            Message::KodiReq(command) => match &mut self.state {
                State::Connected(connection) => {
                    match &command {
                        &KodiCommand::GetSources(_) => {
                            self.item_list.breadcrumb.clear();
                            self.item_list.breadcrumb.push(command.clone());
                            self.content_area = ContentArea::Loading;
                        }
                        &KodiCommand::GetDirectory { .. } => {
                            self.item_list.breadcrumb.push(command.clone());
                            self.content_area = ContentArea::Loading;
                        }
                        _ => {}
                    }
                    connection.send(command);
                }
                State::Disconnected => {
                    println!("TODO: Kodi is disconnected UI state")
                    //panic!("Kodi is apparently disconnected so I can't");
                }
            },

            _ => {}
        }
        Command::none()
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        let mut subs = vec![
            subscription::events_with(|event, _| match event {
                Event::Window(window::Event::Resized { width: _, height }) => {
                    Some(Message::WindowResized(height))
                }
                _ => None,
            }),
            db::connect().map(Message::DbEvent),
        ];
        if let Some(kodi_server) = &self.kodi_status.server {
            subs.push(client::connect(Arc::clone(kodi_server)).map(Message::ServerStatus));
        };
        iced::Subscription::batch(subs)
    }

    fn view(&self) -> Element<Message> {
        if let ContentArea::Settings(set) = &self.content_area {
             // TODO: modify this so that the left_menu is on it stil..
            return set.view().map(Message::Settings);
        };
        let content = column![
            row![
                uiparts::left_menu(self),
                column![
                    // Top Bar thing
                    uiparts::top_bar(self),
                    row![
                        //Center (content)
                        uiparts::center_area(self),
                        // Right (remote)
                        uiparts::remote(self),
                    ],
                ],
            ]
            .height(Length::Fill),
            uiparts::playing_bar(self),
        ];

        let content: Element<_> = container(content).into();

        let modal = match self.modal {
            Modals::Subtitles => Some(uiparts::make_subtitle_modal(self)),
            Modals::RequestText => Some(uiparts::request_text_modal(self)),
            Modals::Audio => Some(uiparts::make_audio_modal(self)),
            _ => None,
        };

        if let Some(modal) = modal {
            Modal::new(content, modal)
                .on_blur(Message::ShowModal(Modals::None))
                .into()
        } else {
            content
        }
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

    async fn download_pic(pic: Pic) -> Result<image::Handle, Box<dyn Error>> {
        let img = reqwest::get(pic.url).await?;
        let img = img.bytes().await?;

        let img = imagelib::load_from_memory(&img)?;
        let img = img.resize_to_fill(pic.w, pic.h, imagelib::imageops::FilterType::Nearest);
        let img = img.to_rgba8().to_vec();

        Ok(image::Handle::from_pixels(pic.w, pic.h, img))
    }

    fn handle_server_event(&mut self, event: client::Event) -> Option<Command<Message>> {
        match event {
            client::Event::Connected(connection) => {
                self.state = State::Connected(connection);
            }

            client::Event::Disconnected => {
                self.state = State::Disconnected;
            }

            client::Event::UpdateDirList(dirlist) => {
                let sem = Arc::new(Semaphore::new(10));
                let mut files: Vec<ListData> = Vec::new();
                let http_url = if let Some(server) = &self.kodi_status.server {
                    server.http_url()
                } else {
                    panic!("Shouldn't be called if there's no server")
                };
                for file in dirlist {
                    let pic = get_art_url(&file, &http_url);
                    let pic = get_art(&sem, pic);

                    let mut item: ListData = file.into();
                    item.image = pic;
                    files.push(item);
                }
                self.item_list.data = files;

                self.item_list.filter = "".to_string();
                self.item_list.start_offset = 0;
                self.content_area = ContentArea::Files;
                return Some(scrollable::snap_to(
                    Id::new("files"),
                    scrollable::RelativeOffset { x: 0.0, y: 0.0 },
                ));
            }

            client::Event::UpdateSources(sources) => {
                let mut files: Vec<ListData> = Vec::new();
                for source in sources {
                    files.push(source.into())
                }
                self.item_list.data = files;

                self.item_list.filter = "".to_string();
                self.item_list.start_offset = 0;
                self.content_area = ContentArea::Files;
                return Some(scrollable::snap_to(
                    Id::new("files"),
                    scrollable::RelativeOffset { x: 0.0, y: 0.0 },
                ));
            }

            client::Event::UpdatePlayerProps(player_props) => match player_props {
                None => {
                    self.kodi_status.active_player_id = None;
                }
                Some(props) => {
                    if self.kodi_status.active_player_id.is_none() {
                        self.kodi_status.active_player_id = props.player_id;
                        let player_id = props.player_id.expect("player_id should exist");
                        return Some(Command::perform(async {}, move |_| {
                            Message::KodiReq(KodiCommand::PlayerGetPlayingItem(player_id))
                        }));
                    }
                    self.kodi_status.active_player_id = props.player_id;

                    // Not sure I like this. might add a playbar_position type thing.
                    if !self.slider_grabbed {
                        self.kodi_status.player_props = props;
                    } else {
                        let selected_time = self.kodi_status.player_props.time.clone();
                        self.kodi_status.player_props = props;
                        self.kodi_status.player_props.time = selected_time;
                    }
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

            client::Event::InputRequested(input) => {
                self.send_text = input;
                self.modal = Modals::RequestText;
            }

            client::Event::None => {}
        }
        None
    }
}

struct Pic {
    url: String,
    h: u32,
    w: u32,
}

fn get_art(sem: &Arc<Semaphore>, pic: Pic) -> Arc<OnceLock<image::Handle>> {
    if !pic.url.is_empty() {
        // This semaphore limits it to 10 hits on the server at a time.
        let permit = Arc::clone(sem).acquire_owned();
        let lock = Arc::new(OnceLock::new());
        let c_lock = Arc::clone(&lock);
        tokio::spawn(async move {
            let _permit = permit.await;
            let res = Krustmote::download_pic(pic).await;
            if let Ok(res) = res {
                let _ = c_lock.set(res);
            } else {
                dbg!(res.err());
            };
        });
        lock
    } else {
        Arc::new(OnceLock::new())
    }
}

fn get_art_url(file: &DirList, http_url: &String) -> Pic {
    if file.type_ == VideoType::Episode && file.art.thumb.is_some() {
        let thumb = file.art.thumb.as_ref().unwrap();
        let thumb = urlencoding::encode(thumb.as_str());
        Pic {
            url: format!("{}/image/{}", http_url, thumb),
            w: 192,
            h: 108,
        }
    } else if file.art.poster.is_some() {
        let poster = file.art.poster.as_ref().unwrap();
        let poster = urlencoding::encode(poster.as_str());
        Pic {
            url: format!("{}/image/{}", http_url, poster),
            w: 80,
            h: 120,
        }
    } else {
        Pic {
            url: "".to_string(),
            h: 0,
            w: 0,
        }
    }
}
