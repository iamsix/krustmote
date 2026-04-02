#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// use iced::widget::scrollable::Id;
use iced::widget::{center, column, container, image, mouse_area, row, scrollable, stack};
use iced::widget::{opaque, operation};

use iced::{Element, Event, Length, Subscription, Task as Command, event, font, window};

use ::image as imagelib;
use reqwest;
use std::path::Path;
use tokio::fs;
use tokio::sync::Semaphore;

use directories_next::ProjectDirs;
use indexmap::IndexMap;
use std::env;
use std::error::Error;
use std::fs as stdfs;
use std::sync::Mutex;
use std::sync::{Arc, LazyLock, OnceLock};
use tokio;
use tracing::{debug, error, info};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod client;
mod data;
mod db;
mod icons;
mod koditypes;
mod settingsui;
mod themes;
mod uiparts;
mod widgets {
    pub mod listitem;
}

use koditypes::*;

static SEM: Semaphore = Semaphore::const_new(10);
pub const ITEM_HEIGHT: u32 = 55;
const MENU_WIDTH_OPEN: u32 = 120;
const DEFAULT_IMAGE_W: u32 = 80;
const DEFAULT_IMAGE_H: u32 = 120;

static BLANK_IMAGE: OnceLock<image::Handle> = OnceLock::new();
static PROJECT_DIRS: LazyLock<ProjectDirs> = LazyLock::new(|| {
    ProjectDirs::from("ca", "sixis", "Krustmote")
        .expect("Unlikely to ever run on an OS that doesn't support it")
});

static DECODED_IMAGE_CACHE: LazyLock<Mutex<IndexMap<usize, image::Handle>>> =
    LazyLock::new(|| Mutex::new(IndexMap::with_capacity(100)));

fn main() -> iced::Result {
    let icon = include_bytes!("../icon.png");
    let window = window::icon::from_file_data(icon, Some(imagelib::ImageFormat::Png)).map_or(
        window::Settings::default(),
        |icon| window::Settings {
            icon: Some(icon),
            ..Default::default()
        },
    );

    let log_dir = PROJECT_DIRS.data_dir();
    let log_path = log_dir.join("krustmote.log");
    let file = stdfs::File::create(log_path).expect("failed to create log file");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file);

    let filter_str = match env::var("RUST_LOG").map(|s| s.to_lowercase()) {
        Ok(s) if s == "debug" => "krustmote=debug,jsonrpsee=debug",
        _ => "krustmote=info,jsonrpsee=info",
    };

    let filter = EnvFilter::new(filter_str);

    tracing_subscriber::registry()
        .with(
            fmt::layer().with_filter(filter.clone()), // Stdout uses EnvFilter
        )
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_writer(non_blocking)
                .with_filter(filter), // File also uses EnvFilter
        )
        .init();

    let _ = BLANK_IMAGE.set(image::Handle::from_rgba(
        DEFAULT_IMAGE_W,
        DEFAULT_IMAGE_H,
        vec![0; (DEFAULT_IMAGE_W * DEFAULT_IMAGE_H * 4) as usize],
    ));
    iced::application(Krustmote::new, Krustmote::update, Krustmote::view)
        .subscription(Krustmote::subscription)
        .window(window)
        .title(Krustmote::title)
        // .theme(Krustmote::theme)
        .run()
}

struct Krustmote {
    state: State,
    menu_width: u32,
    kodi_status: KodiStatus,
    item_list: ItemList,
    slider_grabbed: bool,
    send_text: String,
    content_area: ContentArea,
    modal: Modals,
}

#[derive(Default)]
struct ItemList {
    raw_data: Vec<Box<dyn IntoListData + Send>>,
    filtered_indices: Vec<usize>,
    virtual_list: IndexMap<usize, ListData>,
    list_title: String,
    breadcrumb: Vec<Message>,
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

// TODO: consider directly using PlayerProps and PlayingItem
//       this basically just re-makes those structs anyway...
#[derive(Debug, Clone, Default)]
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
    label: Arc<str>,
    on_click: Message,
    play_count: Option<i16>,
    // content_area: Option<String>, // container/element instead?
    bottom_left: Option<String>,  // container/element?
    bottom_right: Option<String>, // container/element?
    image: Option<image::Handle>,
}

#[derive(Debug, Clone)]
enum Message {
    ToggleLeftMenu,
    UpBreadCrumb,
    KodiReq(KodiCommand),
    DataEvent(data::DataEvent),
    GetData(data::Get),
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
    ImageLoaded { index: usize, handle: image::Handle },
    None,
}

#[derive(Debug)]
enum State {
    Disconnected,
    Offline(data::Connection),
    Connected(data::Connection, client::Connection),
}

impl Krustmote {
    fn new() -> (Self, Command<Message>) {
        let res = make_cache_dir();
        if res.is_err() {
            error!("Failed to create cache directory: {:?}", res.err());
        }
        (
            Self {
                state: State::Disconnected,
                menu_width: MENU_WIDTH_OPEN,
                kodi_status: Default::default(),
                item_list: Default::default(),
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

    // fn theme(&self) -> Theme {
    //     Theme::CatppuccinMocha
    // }

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
                    self.content_area = ContentArea::Files;
                    let q = data::Get::AddOrEditServer(srv);
                    return Command::perform(async { q }, move |q| Message::GetData(q));
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
                self.menu_width = if self.menu_width == 0 {
                    MENU_WIDTH_OPEN
                } else {
                    0
                };
            }

            Message::HideModalAndKodiReq(cmd) => {
                self.modal = Modals::None;

                if matches!(cmd, KodiCommand::InputSendText(_)) {
                    self.send_text = "".to_string();
                }

                return Command::perform(async { cmd }, move |cmd| Message::KodiReq(cmd));
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
                return Command::perform(async { cmd }, move |c| c);
            }

            Message::Scrolled(view) => {
                let old = self.item_list.start_offset;
                let offset = (view.absolute_offset().y / ITEM_HEIGHT as f32) as u32;
                self.item_list.start_offset = offset.saturating_sub(1);

                if old != self.item_list.start_offset {
                    return self.update_virtual_list();
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
                return Command::perform(async { cmd }, move |c| Message::KodiReq(c));
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
                return Command::perform(async { cmd }, move |c| Message::KodiReq(c));
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
                return Command::perform(async { cmd }, move |c| Message::KodiReq(c));
            }

            Message::SendTextInput(text) => {
                self.send_text = text;
            }

            Message::FilterFileList(filter) => {
                let mut cmds = vec![operation::snap_to(
                    "files",
                    scrollable::RelativeOffset { x: 0.0, y: 0.0 },
                )];
                if filter.is_empty() {
                    cmds.push(operation::focus("Filter"))
                }

                self.item_list.filter = filter;
                self.item_list.start_offset = 0;
                self.item_list.virtual_list = IndexMap::new();

                self.recompute_filter();
                let art_task = self.update_virtual_list();

                return Command::batch(vec![Command::batch(cmds), art_task]);
            }

            Message::ImageLoaded { index, handle } => {
                if let Some(item) = self.item_list.virtual_list.get_mut(&index) {
                    item.image = Some(handle);
                }
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
                return Command::perform(async { cmd }, move |c| Message::KodiReq(c));
            }

            Message::DataEvent(event) => {
                return self.handle_data_event(event);
            }

            Message::GetData(cmd) => {
                return self.handle_get_data(cmd);
            }

            Message::KodiReq(command) => match &mut self.state {
                State::Connected(_, connection) => {
                    connection.send(command);
                }

                State::Offline(_) | State::Disconnected => {
                    println!("TODO: Kodi is disconnected UI state")
                    //panic!("Kodi is apparently disconnected so I can't");
                }
            },

            _ => {}
        }

        Command::none()
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        let subs = vec![
            event::listen_with(|mevent, _, _| match mevent {
                Event::Window(window::Event::Resized(sz)) => {
                    Some(Message::WindowResized(sz.height as u32))
                }
                _ => None,
            }),
            Subscription::run(data::connect).map(Message::DataEvent),
        ];

        iced::Subscription::batch(subs)
    }

    fn view(&'_ self) -> Element<'_, Message> {
        if let ContentArea::Settings(set) = &self.content_area {
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

    fn up_breadcrumb(&mut self) -> Message {
        // dbg!(&self.breadcrumb);
        let _ = self.item_list.breadcrumb.pop();
        let command = self.item_list.breadcrumb.pop();
        command.expect("List should have an entry if this is callable")
    }

    fn handle_data_event(&mut self, event: data::DataEvent) -> Command<Message> {
        match event {
            data::DataEvent::Offline(connection) => {
                self.kodi_status.active_player_id = None;
                self.state = State::Offline(connection);
                Command::perform(async {}, |_| Message::GetData(data::Get::KodiServers))
            }
            data::DataEvent::Online(conn, kodiconn) => {
                self.state = State::Connected(conn, kodiconn);
                Command::none()
            }
            data::DataEvent::Servers(servers) => {
                if servers.is_empty() {
                    // Only switch to or reset settings if we aren't already there.
                    // This prevents background reconnect cycles from clobbering active input.
                    if !matches!(self.content_area, ContentArea::Settings(_)) {
                        let new_server = settingsui::Settings::new();
                        self.content_area = ContentArea::Settings(new_server);
                    }
                } else {
                    self.kodi_status.server = Some(Arc::new(servers[0].clone()));
                }
                Command::none()
            }
            data::DataEvent::ListData {
                request,
                title,
                data,
            } => {
                // Check if this data matches our current focus.
                // If we aren't "Loading" and the request doesn't match the top of the breadcrumb,
                // this is a background sync result for a view we've navigated away from.
                if let Some(Message::GetData(current_req)) = self.item_list.breadcrumb.last() {
                    if let (data::Get::TVEpisodes(s1, e1, _), data::Get::TVEpisodes(s2, e2, _)) =
                        (current_req, &request)
                    {
                        if s1 == s2 && e1 != e2 && *e2 == -1 {
                            // Background sync for the show finished, but we are viewing a specific season.
                            // Trigger a refresh of our specific season instead of accepting "all seasons" data.
                            return Command::perform(
                                {
                                    let r = current_req.clone();
                                    async move { r }
                                },
                                Message::GetData,
                            );
                        }
                    }
                }

                let mut matches_breadcrumb = false;
                if let Some(Message::GetData(current_req)) = self.item_list.breadcrumb.last() {
                    matches_breadcrumb = match (current_req, &request) {
                        (data::Get::Movies(_), data::Get::Movies(_)) => true,
                        (data::Get::TVShows(_), data::Get::TVShows(_)) => true,
                        (data::Get::TVEpisodes(s1, e1, _), data::Get::TVEpisodes(s2, e2, _)) => {
                            s1 == s2 && e1 == e2
                        }
                        _ => current_req == &request,
                    };
                }

                if !matches_breadcrumb && !matches!(self.content_area, ContentArea::Loading) {
                    return Command::none();
                }

                self.item_list.list_title = title;
                self.item_list.raw_data = data;
                self.item_list.filter = String::new();
                self.item_list.start_offset = 0;
                self.item_list.virtual_list.clear();

                self.recompute_filter();
                let art_task = self.update_virtual_list();
                self.content_area = ContentArea::Files;

                Command::batch(vec![
                    art_task,
                    operation::snap_to("files", scrollable::RelativeOffset { x: 0.0, y: 0.0 }),
                ])
            }
            data::DataEvent::KodiStatus(kodistatus) => {
                if !self.slider_grabbed {
                    self.kodi_status = kodistatus;
                } else {
                    let selected_time = self.kodi_status.player_props.time.clone();
                    self.kodi_status = kodistatus;
                    self.kodi_status.player_props.time = selected_time;
                }
                Command::none()
            }
            data::DataEvent::InputRequested(input) => {
                self.send_text = input;
                self.modal = Modals::RequestText;
                Command::none()
            }
        }
    }

    fn handle_get_data(&mut self, cmd: data::Get) -> Command<Message> {
        match &mut self.state {
            State::Connected(connection, _) | State::Offline(connection) => {
                let is_duplicate = self
                    .item_list
                    .breadcrumb
                    .last()
                    .map(|msg| {
                        if let Message::GetData(existing_cmd) = msg {
                            existing_cmd == &cmd
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);

                match &cmd {
                    data::Get::Movies(sync) | data::Get::TVShows(sync) => {
                        if *sync {
                            if !is_duplicate {
                                self.item_list.breadcrumb.clear();
                                self.item_list
                                    .breadcrumb
                                    .push(Message::GetData(cmd.clone()));
                            }
                            self.content_area = ContentArea::Loading;
                        }
                    }
                    data::Get::Sources => {
                        if !is_duplicate {
                            self.item_list.breadcrumb.clear();
                            self.item_list
                                .breadcrumb
                                .push(Message::GetData(cmd.clone()));
                        }
                        self.content_area = ContentArea::Loading;
                    }
                    data::Get::TVEpisodes(_, _, sync) => {
                        if *sync {
                            if !is_duplicate {
                                self.item_list
                                    .breadcrumb
                                    .push(Message::GetData(cmd.clone()));
                            }
                            self.content_area = ContentArea::Loading;
                        }
                    }
                    data::Get::TVSeasons(_) | data::Get::Directory { .. } => {
                        if !is_duplicate {
                            self.item_list
                                .breadcrumb
                                .push(Message::GetData(cmd.clone()));
                        }
                        self.content_area = ContentArea::Loading;
                    }
                    _ => {}
                }
                connection.send(cmd);
            }
            _ => {}
        }
        Command::none()
    }

    fn recompute_filter(&mut self) {
        self.item_list.filtered_indices = self
            .item_list
            .raw_data
            .iter()
            .enumerate()
            .filter(|(_, item)| item.label_contains(&self.item_list.filter))
            .map(|(i, _)| i)
            .collect();
        self.item_list.filtered_count = self.item_list.filtered_indices.len();
    }

    fn update_virtual_list(&mut self) -> Command<Message> {
        let start = self.item_list.start_offset as usize;
        let end = (self.item_list.start_offset + self.item_list.visible_count) as usize;

        let mut tasks = Vec::new();

        // Remove items no longer visible
        self.item_list
            .virtual_list
            .retain(|&i, _| i >= start && i <= end);

        // Add newly visible items
        for i in start..=end {
            if i < self.item_list.filtered_indices.len()
                && !self.item_list.virtual_list.contains_key(&i)
            {
                let raw_idx = self.item_list.filtered_indices[i];
                let file = &self.item_list.raw_data[raw_idx];

                let mut item = file.into_listdata();
                let pic = file.get_art_data(&self.kodi_status.server);

                // Immediate check for memory cache
                if let Ok(cache) = DECODED_IMAGE_CACHE.lock() {
                    if let Some(handle) = cache.get(&pic.namehash) {
                        item.image = Some(handle.clone());
                    }
                }

                // If not in cache, trigger background task
                if item.image.is_none() {
                    tasks.push(self.load_art_task(i, pic));
                }

                self.item_list.virtual_list.insert(i, item);
            }
        }
        self.item_list.virtual_list.sort_keys();
        Command::batch(tasks)
    }

    fn load_art_task(&self, index: usize, pic: Pic) -> Command<Message> {
        if pic.url.is_none() && pic.namehash == 0 {
            return Command::none();
        }

        let online = matches!(self.state, State::Connected(_, _));

        Command::future(async move {
            let namehash = pic.namehash;
            let path = PROJECT_DIRS
                .cache_dir()
                .join(format!("{:0x}.jpg", namehash));

            let res = match Krustmote::cache_hit(&path).await {
                Ok(val) => Ok(val),
                Err(_) => {
                    if online && pic.url.is_some() {
                        // semaphore limits it to 10 simultaneous DLs from svr
                        let _permit = SEM.acquire().await;
                        Krustmote::download_pic(pic, &path).await
                    } else {
                        return Message::None;
                    }
                }
            };

            if let Ok(res) = res {
                if let Ok(mut cache) = DECODED_IMAGE_CACHE.lock() {
                    cache.insert(namehash, res.clone());
                }
                Message::ImageLoaded { index, handle: res }
            } else if let Err(err) = res {
                error!("Art task error: {:?}", err);
                Message::None
            } else {
                Message::None
            }
        })
    }

    async fn cache_hit(path: &Path) -> Result<image::Handle, Box<dyn Error + Send + Sync>> {
        let path = if fs::metadata(path).await.is_ok() {
            path
        } else if fs::metadata(path.with_extension("png")).await.is_ok() {
            &path.with_extension("png")
        } else {
            return Err("No cache hit".into());
        };

        Ok(image::Handle::from_path(path))
    }

    async fn download_pic(
        pic: Pic,
        cache_path: &Path,
    ) -> Result<image::Handle, Box<dyn Error + Send + Sync>> {
        let url = pic.url.expect("Must exist if gotten here");
        let img = reqwest::get(url).await?.error_for_status()?;
        let img = img.bytes().await?;

        let fmt = imagelib::guess_format(&img)?;
        let path = match fmt {
            imagelib::ImageFormat::Jpeg => cache_path.with_extension("jpg"),
            imagelib::ImageFormat::Png => cache_path.with_extension("png"),
            _ => {
                panic!("Unknwown format {:?}", fmt)
            }
        };

        let img = imagelib::load_from_memory(&img)?;
        let img = img.resize_to_fill(pic.w, pic.h, imagelib::imageops::FilterType::Nearest);

        img.save(path)?;

        let img = img.into_rgba8().to_vec();
        Ok(image::Handle::from_rgba(pic.w, pic.h, img))
    }
}

fn make_cache_dir() -> Result<(), Box<dyn Error>> {
    let meta = stdfs::metadata(PROJECT_DIRS.cache_dir());
    if meta.is_err() {
        stdfs::create_dir_all(PROJECT_DIRS.cache_dir())?
    } else {
        if !meta?.is_dir() {
            panic!(
                "{:?} exists but is not a directory",
                PROJECT_DIRS.cache_dir()
            )
        };
    }
    Ok(())
}
