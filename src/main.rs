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
mod icons;
mod koditypes;
mod modal;
mod uiparts;

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
    active_player_id: Option<u8>,
    muted: bool,
    paused: bool,
    playing_title: String,
    play_time: KodiTime,
    duration: KodiTime,
    current_subtitle: Option<Subtitle>,
    subtitles: Vec<Subtitle>,
    subtitles_enabled: bool,
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
    SubtitlePicked(Subtitle),
    SubtitleEnable(bool),
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
            active_player_id: None,
            muted: false,
            paused: false,
            playing_title: String::from(""),
            play_time: Default::default(),
            duration: Default::default(),
            current_subtitle: None,
            subtitles: Vec::new(),
            subtitles_enabled: false,
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

            Message::SubtitlePicked(sub) => {
                let cmd = KodiCommand::SetSubtitle {
                    player_id: self.kodi_status.active_player_id.unwrap(),
                    subtitle_index: sub.index,
                    enabled: self.kodi_status.subtitles_enabled,
                };
                return Command::perform(async {}, |_| Message::KodiReq(cmd));
            }

            Message::SubtitleEnable(val) => {
                dbg!(val);
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
                let cmd = KodiCommand::PlayerSeek(
                    self.kodi_status
                        .active_player_id
                        .expect("should have a player_id if this is visible"),
                    self.kodi_status.play_time.clone(),
                );
                return Command::perform(async {}, |_| Message::KodiReq(cmd));
            }

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
        let content = column![
            // Top Bar thing
            uiparts::top_bar(self),
            row![
                // Left (menu)
                uiparts::left_menu(self), //.explain(Color::from_rgb8(0, 255, 0)),
                //Center (content)
                uiparts::center_area(self),
                // Right (remote)
                uiparts::remote(self),
            ]
            .height(Length::Fill),
            uiparts::playing_bar(self),
        ];

        let content: Element<_> = container(content).into();

        match self.modal {
            Modals::Subtitles => {
                // TODO: offload this subtitles dialog elswhere.
                let modal = uiparts::make_subtitle_modal(self);
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

    fn make_itemlist(&mut self, dirlist: Vec<DirList>) -> Option<Command<Message>> {
        self.item_list.filter = "".to_string();
        self.item_list.start_offset = 0;
        let sem = Arc::new(Semaphore::new(10));
        let mut files: Vec<ListData> = Vec::new();
        for file in dirlist {
            // dbg!(&file);

            let (pic_url, w, h) = get_art_url(&file);
            let pic = get_art(&sem, pic_url, h, w);

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

            let bottom_left = if file.size > 1_073_741_824 {
                Some(format!(
                    "{:.2} GB",
                    (file.size as f64 / 1024.0 / 1024.0 / 1024.0)
                ))
            } else if file.size > 0 {
                Some(format!("{:.1} MB", (file.size as f64 / 1024.0 / 1024.0)))
            } else {
                None
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
                bottom_left,
                image: pic,
            })

            // pic.await;
        }
        self.item_list.data = files;
        return Some(scrollable::snap_to(
            Id::new("files"),
            scrollable::RelativeOffset { x: 0.0, y: 0.0 },
        ));
    }

    fn make_sources(&mut self, sources: Vec<Sources>) -> Option<Command<Message>> {
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
        return Some(scrollable::snap_to(
            Id::new("files"),
            scrollable::RelativeOffset { x: 0.0, y: 0.0 },
        ));
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
                if let Some(value) = self.make_itemlist(dirlist) {
                    return Some(value);
                }
            }

            client::Event::UpdateSources(sources) => {
                if let Some(value) = self.make_sources(sources) {
                    return Some(value);
                }
            }

            client::Event::UpdatePlayerProps(player_props) => match player_props {
                None => {
                    self.kodi_status.now_playing = false;
                    self.kodi_status.active_player_id = None;
                }
                Some(props) => {
                    self.kodi_status.active_player_id = props.player_id;
                    if !self.kodi_status.now_playing {
                        self.kodi_status.now_playing = true;
                        let player_id = props.player_id.expect("player_id should exist");
                        return Some(Command::perform(async {}, move |_| {
                            Message::KodiReq(KodiCommand::PlayerGetPlayingItem(player_id))
                        }));
                    }
                    self.kodi_status.now_playing = true;
                    self.kodi_status.paused = props.speed == 0.0;
                    if props.currentsubtitle.is_some() {
                        self.kodi_status.subtitles = props.subtitles;
                        self.kodi_status.current_subtitle = props.currentsubtitle;
                    }
                    self.kodi_status.subtitles_enabled = props.subtitleenabled;

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
        }
        None
    }
}

fn get_art(sem: &Arc<Semaphore>, pic_url: String, h: u32, w: u32) -> Arc<OnceLock<image::Handle>> {
    let lock = Arc::new(OnceLock::new());
    let c_lock = Arc::clone(&lock);
    // This semaphore limits it to 10 hits on the server at a time.
    let permit = Arc::clone(sem).acquire_owned();

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
    pic
}

fn get_art_url(file: &DirList) -> (String, u32, u32) {
    let (pic_url, w, h) = if file.type_ == VideoType::Episode && file.art.thumb.is_some() {
        let thumb = file.art.thumb.as_ref().unwrap();
        let thumb = urlencoding::encode(thumb.as_str());
        (
            format!("http://192.168.1.22:8080/image/{}", thumb),
            192,
            108,
        )
    } else if file.art.poster.is_some() {
        let poster = file.art.poster.as_ref().unwrap();
        let poster = urlencoding::encode(poster.as_str());
        (
            format!("http://192.168.1.22:8080/image/{}", poster),
            80,
            120,
        )
    } else {
        ("".to_string(), 0, 0)
    };
    (pic_url, w, h)
}
