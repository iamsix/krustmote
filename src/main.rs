use iced::widget::scrollable::Id;
use iced::widget::{center, column, container, image, mouse_area, row, scrollable, stack};
use iced::widget::{opaque, text_input};

use iced::{Element, Event, Length, Subscription, Task as Command, event, font, window};

use ::image as imagelib;
use reqwest;
use std::path::Path;
use tokio::fs;
use tokio::sync::Semaphore;

use directories_next::ProjectDirs;
use indexmap::IndexMap;
use std::error::Error;
use std::sync::{Arc, LazyLock, OnceLock};
use tokio;

mod client;
mod data;
mod db;
mod icons;
mod koditypes;
mod settingsui;
mod themes;
mod uiparts;

use koditypes::*;

static SEM: Semaphore = Semaphore::const_new(10);
const ITEM_HEIGHT: u32 = 55;
static BLANK_IMAGE: OnceLock<image::Handle> = OnceLock::new();
static PROJECT_DIRS: LazyLock<ProjectDirs> = LazyLock::new(|| {
    ProjectDirs::from("ca", "sixis", "Krustmote")
        .expect("Unlikely to ever run on an OS that doesn't support it")
});

fn main() -> iced::Result {
    let icon = include_bytes!("../icon.png");
    let window = window::icon::from_file_data(
        icon,
        Some(iced::advanced::graphics::image::image_rs::ImageFormat::Png),
    )
    .map_or(window::Settings::default(), |icon| window::Settings {
        icon: Some(icon),
        ..Default::default()
    });

    let _ = BLANK_IMAGE.set(image::Handle::from_rgba(80, 120, vec![0; 38_400]));
    iced::application(Krustmote::title, Krustmote::update, Krustmote::view)
        .subscription(Krustmote::subscription)
        .window(window)
        .run_with(Krustmote::new)
}

struct Krustmote {
    state: State,
    menu_width: u16,
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
}

#[derive(Debug)]
enum State {
    Disconnected,
    Offline(data::Connection),
    Connected(data::Connection, client::Connection),
}

impl Krustmote {
    fn new() -> (Self, Command<Message>) {
        (
            Self {
                state: State::Disconnected,
                menu_width: 120,
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
                self.menu_width = if self.menu_width == 0 { 120 } else { 0 };
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
                return Command::perform(async { cmd }, move |c| Message::KodiReq(c));
            }

            Message::DataEvent(event) => {
                // dbg!(&event);
                match event {
                    data::DataEvent::Offline(connection) => {
                        self.kodi_status.active_player_id = None;
                        self.state = State::Offline(connection);

                        return Command::perform(async {}, |_| {
                            Message::GetData(data::Get::KodiServers)
                        });
                    }

                    data::DataEvent::Online(conn, kodiconn) => {
                        self.state = State::Connected(conn, kodiconn);
                    }

                    data::DataEvent::Servers(servers) => {
                        // dbg!(&servers);
                        if servers.len() == 0 {
                            let new_server = settingsui::Settings::new();
                            self.content_area = ContentArea::Settings(new_server);
                        } else {
                            // We currently only care about 1 server until we
                            // have the settings table to get the selected server
                            self.kodi_status.server = Some(Arc::new(servers[0].clone()));
                            // self.content_area = ContentArea::Files;
                        }
                    }

                    data::DataEvent::ListData { title, data } => {
                        if data.is_empty() {
                            dbg!("Empty list:", &title);
                        }

                        self.item_list.list_title = title;
                        self.item_list.raw_data = data;

                        self.item_list.filter = "".to_string();
                        self.item_list.start_offset = 0;

                        self.item_list.virtual_list = IndexMap::new();
                        self.update_virtual_list();

                        self.content_area = ContentArea::Files;

                        return scrollable::snap_to(
                            Id::new("files"),
                            scrollable::RelativeOffset { x: 0.0, y: 0.0 },
                        );
                    }

                    data::DataEvent::KodiStatus(kodistatus) => {
                        if !self.slider_grabbed {
                            self.kodi_status = kodistatus;
                        } else {
                            let selected_time = self.kodi_status.player_props.time.clone();
                            self.kodi_status = kodistatus;
                            self.kodi_status.player_props.time = selected_time;
                        }
                    }

                    data::DataEvent::InputRequested(input) => {
                        self.send_text = input;
                        self.modal = Modals::RequestText;
                    }
                }
            }

            Message::GetData(cmd) => match &mut self.state {
                State::Connected(connection, _) | State::Offline(connection) => {
                    match &cmd {
                        data::Get::Movies | data::Get::TVShows | data::Get::Sources => {
                            self.item_list.breadcrumb.clear();
                            self.item_list
                                .breadcrumb
                                .push(Message::GetData(cmd.clone()));
                            self.content_area = ContentArea::Loading;
                        }
                        data::Get::TVEpisodes(_, _)
                        | data::Get::TVSeasons(_)
                        | data::Get::Directory { .. } => {
                            self.item_list
                                .breadcrumb
                                .push(Message::GetData(cmd.clone()));
                            self.content_area = ContentArea::Loading;
                        }
                        _ => {}
                    }
                    connection.send(cmd);
                }
                _ => {}
            },

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

    fn view(&self) -> Element<Message> {
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

    fn update_virtual_list(&mut self) {
        let sem = Arc::new(&SEM);

        self.item_list.filtered_count = 0;
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
                let pic = file.get_art_data(&self.kodi_status.server);
                let pic = self.get_art(&sem, pic);
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

    // This entire thing might be better using worker oneshots/etc
    // but iced 0.14 will have 'Straw' which is perfect for something like this
    fn get_art(&self, sem: &Arc<&'static Semaphore>, pic: Pic) -> Arc<OnceLock<image::Handle>> {
        let online = matches!(self.state, State::Connected(_, _));
        if pic.url.is_some() {
            // Check cache hit before semaphore await for possible 'early return'
            // This semaphore limits it to 10 hits on the server at a time.
            // Note this permit doesn't await yet, had to define here for async move.
            let permit = Arc::clone(sem).acquire(); // .acquire_owned();
            let lock = Arc::new(OnceLock::new());
            let c_lock = Arc::clone(&lock);

            tokio::spawn(async move {
                let c_path = PROJECT_DIRS.cache_dir();
                let path = if fs::metadata(c_path).await.is_ok() {
                    c_path
                } else {
                    if fs::create_dir_all(c_path).await.is_ok() {
                        c_path
                    } else {
                        // if this one fails it's never going to work.
                        fs::create_dir_all("./imagecache/").await.unwrap();
                        &Path::new("./imagecache/").to_path_buf()
                    }
                };
                let path = path.join(format!("{:0x}.jpg", pic.namehash));
                let res = match Krustmote::cache_hit(&path).await {
                    Ok(val) => Ok(val),
                    Err(_) => {
                        if online {
                            let _permit = permit.await;
                            Krustmote::download_pic(pic, &path).await
                        } else {
                            Err("Not online".into())
                        }
                    }
                };
                if let Ok(res) = res {
                    let _ = c_lock.set(res);
                } else if let Err(err) = res {
                    if err.to_string() != "Not online" {
                        dbg!(err);
                    }
                };
            });
            lock
        } else {
            Arc::new(OnceLock::new())
        }
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
