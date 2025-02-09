use iced::font;
use iced::widget::scrollable::Id;
use iced::widget::{center, column, container, image, mouse_area, row, scrollable, stack};
use iced::widget::{opaque, text_input};

use iced::{event, window, Element, Event, Length, Subscription, Task as Command};

use ::image as imagelib;
use fxhash;
use reqwest;
use std::path::Path;
use tokio::fs;
use tokio::sync::Semaphore;

use indexmap::IndexMap;
use std::error::Error;
use std::sync::{Arc, OnceLock};
use tokio;

mod client;
mod db;
mod icons;
mod koditypes;
// mod modal;
mod settingsui;
mod themes;
mod uiparts;

// use modal::Modal;

use koditypes::*;

static SEM: Semaphore = Semaphore::const_new(10);

fn main() -> iced::Result {
    // let dir = dirs_next::data_dir()
    //     .expect("should have a data dir")
    //     .join("krustmote");
    // if !dir.exists() {
    //     std::fs::create_dir(dir.as_path()).expect("expected permissions to create data folder");
    // }

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

    let _ = BLANK_IMAGE.set(image::Handle::from_rgba(80, 120, vec![0; 38_400]));
    iced::application(Krustmote::title, Krustmote::update, Krustmote::view)
        .subscription(Krustmote::subscription)
        .window(window)
        .run_with(Krustmote::new)

    // Krustmote::run(Settings {
    //     // window,
    //     ..Settings::default()
    // })
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
    raw_data: Vec<Box<dyn IntoListData>>,
    virtual_list: IndexMap<usize, ListData>,
    list_title: String,
    breadcrumb: Vec<KodiCommand>,
    filter: String,
    filtered_count: usize,
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
    play_count: Option<i16>,
    // content_area: Option<String>, // container/element instead?
    bottom_left: Option<String>,  // container/element?
    bottom_right: Option<String>, // container/element?
    image: Arc<OnceLock<image::Handle>>,
}

#[derive(Debug, Clone)]
enum Message {
    ToggleLeftMenu,
    UpBreadCrumb,
    ServerEvent(client::Event),
    KodiReq(KodiCommand),
    DbEvent(db::Event),
    DbQuery(db::SqlCommand),
    Settings(settingsui::Message),
    SettingsEvent(settingsui::Event),
    ShowSettings,
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

#[derive(Debug)]
enum State {
    Disconnected,
    Connected(client::Connection),
}

enum DbState {
    Closed,
    Open(db::SqlConnection),
}

impl Krustmote {
    // type Message = Message;
    // type Theme = Theme;
    // type Executor = executor::Default;
    // type Flags = ();

    fn new() -> (Self, Command<Message>) {
        let kodi_status = KodiStatus {
            server: None,
            active_player_id: None,
            muted: false,
            playing_title: "".to_string(),
            player_props: Default::default(),
            // playing_item: Default::default(),
        };

        let item_list = ItemList {
            // data: Vec::new(),
            raw_data: Vec::new(),
            virtual_list: IndexMap::new(),
            list_title: String::from(""),
            breadcrumb: Vec::new(),
            start_offset: 0,
            visible_count: 0,
            filter: String::from(""),
            filtered_count: 0,
        };
        (
            Self {
                state: State::Disconnected,
                db_state: DbState::Closed,
                menu_width: 120,
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

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Settings(settings_msg) => {
                if let ContentArea::Settings(set) = &mut self.content_area {
                    return set.update(settings_msg).map(Message::SettingsEvent);
                }
            }

            Message::SettingsEvent(event) => match event {
                settingsui::Event::AddServer(srv) => {
                    let q = db::SqlCommand::AddOrEditServer(srv);
                    return Command::perform(async {}, move |_| Message::DbQuery(q.clone()));
                }
                settingsui::Event::Cancel => {
                    self.content_area = ContentArea::Files;
                }
            },

            Message::ShowSettings => {
                let settings = if let Some(server) = &self.kodi_status.server {
                    settingsui::Settings::load(Arc::clone(server))
                } else {
                    settingsui::Settings::new()
                };
                self.content_area = ContentArea::Settings(settings);
            }

            Message::ToggleLeftMenu => {
                // TODO : Fancy animation by subtracting until 0 etc. maybe.
                self.menu_width = if self.menu_width == 0 { 120 } else { 0 };
            }

            Message::HideModalAndKodiReq(cmd) => {
                self.modal = Modals::None;

                if matches!(cmd, KodiCommand::InputSendText(_)) {
                    self.send_text = "".to_string();
                }

                return Command::perform(async {}, move |_| Message::KodiReq(cmd.clone()));
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
                return Command::perform(async {}, move |_| Message::KodiReq(cmd.clone()));
            }

            Message::Scrolled(view) => {
                let old = self.item_list.start_offset;
                let offset = (view.absolute_offset().y / ITEM_HEIGHT as f32) as u32;
                self.item_list.start_offset = offset.saturating_sub(1);

                if old != self.item_list.start_offset {
                    self.update_virtual_list();
                }
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
                return Command::perform(async {}, move |_| Message::KodiReq(cmd.clone()));
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
                return Command::perform(async {}, move |_| Message::KodiReq(cmd.clone()));
                // send SubtitlePicked here with current_subtitle?
            }

            Message::AudioStreamPicked(val) => {
                let cmd = KodiCommand::PlayerSetAudioStream {
                    player_id: self
                        .kodi_status
                        .active_player_id
                        .expect("Should be playing if this is called"),
                    audio_index: val.index,
                };
                return Command::perform(async {}, move |_| Message::KodiReq(cmd.clone()));
            }

            Message::SendTextInput(text) => {
                self.send_text = text;
            }

            Message::FilterFileList(filter) => {
                let mut cmds = vec![scrollable::snap_to(
                    Id::new("files"),
                    scrollable::RelativeOffset { x: 0.0, y: 0.0 },
                )];
                if filter.is_empty() {
                    cmds.push(text_input::focus(text_input::Id::new("Filter")))
                }

                self.item_list.filter = filter;
                self.item_list.start_offset = 0;
                self.item_list.virtual_list = IndexMap::new();
                self.update_virtual_list();

                return Command::batch(cmds);
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
                return Command::perform(async {}, move |_| Message::KodiReq(cmd.clone()));
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
                            let srv = Arc::new(servers[0].clone());
                            self.kodi_status.server = Some(Arc::clone(&srv));
                            self.content_area = ContentArea::Files;
                            // if matches!(self.state, State::Disconnected) {
                            self.kodi_status.active_player_id = None;
                            let cmd = Message::KodiReq(KodiCommand::ChangeServer(Arc::clone(&srv)));
                            return Command::perform(async {}, move |_| cmd.clone());
                            // }
                        }
                    }

                    db::Event::UpdateMovieList(movies) => {
                        let mut commands = vec![scrollable::snap_to(
                            Id::new("files"),
                            scrollable::RelativeOffset { x: 0.0, y: 0.0 },
                        )];
                        if movies.is_empty() {
                            let cmd = Message::KodiReq(KodiCommand::VideoLibraryGetMovies);
                            commands.push(Command::perform(async {}, move |_| cmd.clone()));
                        }

                        self.item_list.list_title = "Movies".to_string();
                        self.item_list.breadcrumb.clear();
                        self.item_list.raw_data =
                            movies.into_iter().map(|v| Box::new(v) as _).collect();

                        self.item_list.filter = "".to_string();
                        self.item_list.start_offset = 0;

                        self.item_list.virtual_list = IndexMap::new();
                        self.update_virtual_list();

                        self.content_area = ContentArea::Files;
                        return Command::batch(commands);
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

            Message::ServerEvent(event) => {
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

    fn subscription(&self) -> iced::Subscription<Message> {
        let mut subs = vec![
            event::listen_with(|mevent, _, _| match mevent {
                Event::Window(window::Event::Resized(sz)) => {
                    Some(Message::WindowResized(sz.height as u32))
                }
                _ => None,
            }),
            Subscription::run(db::connect).map(Message::DbEvent),
        ];
        if let Some(kodi_server) = &self.kodi_status.server {
            subs.push(
                Subscription::run_with_id(42, client::connect(Arc::clone(kodi_server)))
                    .map(Message::ServerEvent),
            );
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
            stack![
                content,
                opaque(
                    mouse_area(center(opaque(modal))).on_press(Message::ShowModal(Modals::None))
                )
            ]
            .into()
        } else {
            content
        }
    }
}

impl Krustmote {
    fn up_breadcrumb(&mut self) -> KodiCommand {
        // dbg!(&self.breadcrumb);
        let _ = self.item_list.breadcrumb.pop();
        let command = self.item_list.breadcrumb.pop();
        command.expect("List should have an entry if this is callable")
    }

    // TODO! Proper path support! (if the dir doesn't exist this will fail)
    // I wanted this to be an option but the imagelib::open returns a result.
    // so it seems easier.
    async fn cache_hit(path: &Path) -> Result<image::Handle, Box<dyn Error + Send + Sync>> {
        let img = if fs::metadata(path).await.is_ok() {
            imagelib::open(path)?
        } else if fs::metadata(path.with_extension("png")).await.is_ok() {
            imagelib::open(path.with_extension("png"))?
        } else {
            return Err("No cache hit".into());
        };
        let w = img.width();
        let h = img.height();
        let img = img.into_rgba8().to_vec();
        Ok(image::Handle::from_rgba(w, h, img))
    }

    async fn download_pic(
        pic: Pic,
        cache_path: &Path,
    ) -> Result<image::Handle, Box<dyn Error + Send + Sync>> {
        let img = reqwest::get(&pic.url).await?.error_for_status()?;
        let img = img.bytes().await?;

        let img = imagelib::load_from_memory(&img)?;
        let img = img.resize_to_fill(pic.w, pic.h, imagelib::imageops::FilterType::Nearest);
        img.save(cache_path)?;
        let img = img.into_rgba8().to_vec();

        Ok(image::Handle::from_rgba(pic.w, pic.h, img))
    }

    fn handle_server_event(&mut self, event: client::Event) -> Option<Command<Message>> {
        match event {
            client::Event::Connected(connection) => {
                self.state = State::Connected(connection);
            }

            client::Event::Disconnected => {
                self.kodi_status.active_player_id = None;
                self.state = State::Disconnected;
            }

            client::Event::UpdateDirList(dirlist, path) => {
                self.item_list.list_title = path;
                self.item_list.raw_data = dirlist.into_iter().map(|v| Box::new(v) as _).collect();

                self.item_list.filter = "".to_string();
                self.item_list.start_offset = 0;

                self.item_list.virtual_list = IndexMap::new();
                self.update_virtual_list();

                self.content_area = ContentArea::Files;
                return Some(scrollable::snap_to(
                    Id::new("files"),
                    scrollable::RelativeOffset { x: 0.0, y: 0.0 },
                ));
            }

            client::Event::UpdateSources(sources) => {
                self.item_list.list_title = "Sources".to_string();
                self.item_list.raw_data = sources.into_iter().map(|v| Box::new(v) as _).collect();
                self.item_list.filter = "".to_string();
                self.item_list.start_offset = 0;

                self.item_list.virtual_list = IndexMap::new();
                self.update_virtual_list();

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
                self.kodi_status.playing_title = item.make_title();
            }

            client::Event::InputRequested(input) => {
                self.send_text = input;
                self.modal = Modals::RequestText;
            }

            client::Event::UpdateMovieList(movies) => {
                // panic!("Can't do this rn");
                let cmd = Message::DbQuery(db::SqlCommand::InsertMovies(movies));
                return Some(Command::perform(async {}, move |_| cmd.clone()));
            }

            client::Event::None => {}
        }
        None
    }

    fn update_virtual_list(&mut self) {
        let sem = Arc::new(&SEM);
        let http_url = if let Some(server) = &self.kodi_status.server {
            server.http_url()
        } else {
            panic!("This should never be called if there's no server")
        };

        self.item_list.filtered_count = 0;
        //self.item_list.virtual_list = Vec::new();
        for (i, file) in self
            .item_list
            .raw_data
            .iter()
            .filter(|i| i.label_contains(&self.item_list.filter))
            .enumerate()
        {
            self.item_list.filtered_count += 1;
            //let i = i.clone();
            if i >= self.item_list.start_offset as usize
                && i <= (self.item_list.visible_count + self.item_list.start_offset) as usize
            {
                if self.item_list.virtual_list.contains_key(&i) {
                    continue;
                }
                let pic = file.get_art_data(&http_url);
                let pic = get_art(&sem, pic);
                let mut item = file.into_listdata();
                item.image = pic;
                self.item_list.virtual_list.insert(i, item);
            } else {
                if self.item_list.virtual_list.contains_key(&i) {
                    self.item_list.virtual_list.shift_remove(&i);
                }
            }
        }
        self.item_list.virtual_list.sort_keys()
    }
}

fn get_art(sem: &Arc<&'static Semaphore>, pic: Pic) -> Arc<OnceLock<image::Handle>> {
    if !pic.url.is_empty() {
        // Check cache hit before semaphore await for possible 'early return'
        // This semaphore limits it to 10 hits on the server at a time.
        let permit = Arc::clone(sem).acquire(); // .acquire_owned();
        let lock = Arc::new(OnceLock::new());
        let c_lock = Arc::clone(&lock);
        tokio::spawn(async move {
            // hashing this WITH server url is actually not ideal
            // as even shared databases will then each have their own cached images
            // should hash the original art URL during get_art_data and make it a field in 'pic'
            // technically even unshared db with same art url should have same pic and cache hit
            let hash = fxhash::hash(&pic.url);
            let path = format!("./imagecache/{:0x}.jpg", hash);
            let path = Path::new(&path);

            let res = Krustmote::cache_hit(&path).await;
            let res = if res.is_ok() {
                res
            } else {
                let _permit = permit.await;
                Krustmote::download_pic(pic, &path).await
            };
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
