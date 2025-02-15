use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::sync::{Arc, OnceLock};

use crate::db::SqlCommand;

// TODO: Investigate Cow for these Strings

// TODO: Massively refactor this file.
//       Lots of duplicated code etc.

#[derive(Debug, Clone, PartialEq)]
pub enum KodiCommand {
    ChangeServer(Arc<KodiServer>),
    GetSources(MediaType), // TODO: SortType
    GetDirectory {
        path: String,
        media_type: MediaType,
    }, // TODO: SortType
    PlayerOpen(String),
    InputButtonEvent {
        button: &'static str,
        keymap: &'static str,
    },
    InputExecuteAction(&'static str),
    ToggleMute,
    GUIActivateWindow(&'static str),
    // change to {} to sync with others that take player_id?
    PlayerSeek(u8, KodiTime),
    PlayerSetSubtitle {
        player_id: u8,
        subtitle_index: u8,
        enabled: bool,
    },
    PlayerToggleSubtitle {
        player_id: u8,
        on_off: &'static str,
    },
    PlayerSetAudioStream {
        player_id: u8,
        audio_index: u8,
    },
    InputSendText(String),

    VideoLibraryGetMovies,
    VideoLibraryGetTVShows,
    VideoLibraryGetTVSeasons,
    VideoLibraryGetTVEpisodes,

    PlayerGetProperties,
    PlayerGetPlayingItem(u8),
    PlayerGetActivePlayers,

    // only used for testing/debug:
    PlayerGetPlayingItemDebug(u8),
    Test,
}

fn treat_error_as_none<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;
    Ok(T::deserialize(value).ok())
}

pub trait IntoListData {
    fn into_listdata(&self) -> crate::ListData;
    fn get_art_data(&self, http_url: &String) -> Pic;
    fn label_contains(&self, find: &String) -> bool;
}

pub struct Pic {
    pub url: String,
    pub h: u32,
    pub w: u32,
}

pub const PLAYER_PROPS: [&'static str; 17] = [
    "audiostreams",
    "canseek",
    "currentaudiostream",
    "currentsubtitle",
    "partymode",
    "playlistid",
    "position",
    "repeat",
    "shuffled",
    "speed",
    "subtitleenabled",
    "subtitles",
    "time",
    "totaltime",
    "type",
    "videostreams",
    "currentvideostream",
];

#[derive(Deserialize, Clone, Debug, Default)]
pub struct PlayerProps {
    pub speed: f64,
    pub time: KodiTime,
    pub totaltime: KodiTime,
    pub player_id: Option<u8>,
    #[serde(deserialize_with = "treat_error_as_none")]
    pub currentaudiostream: Option<AudioStream>,
    pub audiostreams: Vec<AudioStream>,
    pub canseek: bool,
    #[serde(deserialize_with = "treat_error_as_none")]
    pub currentsubtitle: Option<Subtitle>,
    pub subtitles: Vec<Subtitle>,
    // #[serde(deserialize_with = "treat_error_as_none")]
    // pub currentvideostream: VideoStream,
    // pub videostreams: Vec<VideoStream>,
    // playlistid: u8,
    // position: u8, //might rename playlist_position..
    // repeat: String //(could be enum?)
    // shuffled: bool,
    pub subtitleenabled: bool,
    // #[serde(rename = "type")]
    // type_: MediaType,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioStream {
    bitrate: u64,
    channels: u8,
    codec: String,
    pub index: u8,
    isdefault: bool,
    isimpaired: bool,
    isoriginal: bool,
    language: String,
    name: String,
    samplerate: u64,
}

impl std::fmt::Display for AudioStream {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut extras = String::from("");
        if self.isdefault {
            extras = extras + " (default)";
        }
        if self.isoriginal {
            extras = extras + " (original)";
        }
        if self.isimpaired {
            extras = extras + " (described)";
        }
        write!(
            f,
            "{} - {} - {} - {} {}ch {extras}",
            self.index, self.language, self.name, self.codec, self.channels,
        )
    }
}

// #[derive(Deserialize, Clone, Debug, Default)]
// pub struct VideoStream {
//     codec: String,
//     height: u16,
//     index: u8,
//     language: String,
//     name: String,
//     width: u16,
// }

#[derive(Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct Subtitle {
    pub index: u8,
    isdefault: bool,
    isforced: bool,
    isimpaired: bool,
    language: String,
    name: String,
}

impl std::fmt::Display for Subtitle {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut extras = String::from("");
        if self.isdefault {
            extras = extras + " (default)";
        }
        if self.isforced {
            extras = extras + " (forced)";
        }
        if self.isimpaired {
            extras = extras + " [CC]";
        }
        write!(
            f,
            "{} - {} - {}{extras}",
            self.index, self.language, self.name
        )
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct KodiTime {
    pub hours: u8,
    // this SHOULD be a u16
    // docs say the max of `milliseconds` is 999 and min is 0
    // but I once got a return of -166 on this somehow
    pub milliseconds: i16,
    pub minutes: u8,
    pub seconds: u8,
}

impl std::fmt::Display for KodiTime {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{:02}:{:02}:{:02}",
            self.hours, self.minutes, self.seconds
        )
    }
}

impl KodiTime {
    pub fn total_seconds(&self) -> u32 {
        self.seconds as u32 + self.minutes as u32 * 60 + self.hours as u32 * 60 * 60
    }

    pub fn set_from_seconds(&mut self, seconds: u32) {
        self.hours = (seconds / 60 / 60) as u8;
        self.minutes = ((seconds / 60).saturating_sub(self.hours as u32 * 60)) as u8;
        self.seconds = seconds
            .saturating_sub(self.minutes as u32 * 60)
            .saturating_sub(self.hours as u32 * 60 * 60) as u8;
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct KodiAppStatus {
    pub muted: bool,
    //volume: u8,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Video,
    // Music,
    // Pictures,
    // Files,
    // Programs,
}

impl MediaType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::Video => "video",
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct ActivePlayer {
    pub playerid: u8,
    // playertype: String,
    // #[serde(rename = "type")]
    // type_: MediaType,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Sources {
    pub label: String,
    pub file: String,
}

impl IntoListData for Sources {
    fn into_listdata(&self) -> crate::ListData {
        crate::ListData {
            label: self.label.clone(),
            on_click: crate::Message::KodiReq(KodiCommand::GetDirectory {
                path: self.file.clone(),
                media_type: MediaType::Video,
            }),
            play_count: None,
            bottom_right: None,
            bottom_left: None,
            image: Arc::new(OnceLock::new()),
        }
    }

    fn get_art_data(&self, _: &String) -> Pic {
        Pic {
            url: "".to_string(),
            h: 0,
            w: 0,
        }
    }

    fn label_contains(&self, find: &String) -> bool {
        self.label.to_lowercase().contains(&find.to_lowercase())
    }
}

// TODO: SortType that defines these
#[derive(Serialize, Debug)]
pub struct DirSort {
    pub method: &'static str,
    pub order: &'static str,
}

pub const FILE_PROPS: [&'static str; 20] = [
    "title",
    "rating",
    "genre",
    "artist",
    "track",
    "season",
    "episode",
    "year",
    "duration",
    "album",
    "showtitle",
    "playcount",
    "file",
    "mimetype",
    "size",
    "lastmodified",
    "resume",
    "art",
    "runtime",
    "displayartist",
];

// TODO: This will need to be much more extensive
//       in order to cover episode 'files' and movie 'files' etc.
//       For now I'm treating everyhing as a generic directory or file.
#[derive(Deserialize, Debug, Clone)]
pub struct DirList {
    pub file: String,
    pub art: Art,
    pub filetype: String,
    pub label: String,
    pub showtitle: Option<String>,
    pub title: Option<String>,
    pub lastmodified: String,
    pub size: u64,
    pub rating: Option<f64>,
    pub season: Option<i16>,
    pub episode: Option<i16>,
    pub playcount: Option<i16>,
    #[serde(default)]
    pub resume: Option<ResumePoint>,
    pub year: Option<u16>,
    #[serde(rename = "type")]
    pub type_: VideoType,
}

// NOTE: this leaves the image blank for now.
// Could probably fix that by doing Into<Vec<ListData> for Vec<DirList>
impl IntoListData for DirList {
    fn into_listdata(&self) -> crate::ListData {
        let label = if self.type_ == VideoType::Episode {
            format!(
                "{} - S{:02}E{:02} - {}",
                self.showtitle.clone().unwrap_or("".to_string()),
                self.season.unwrap_or(0),
                self.episode.unwrap_or(0),
                self.title.clone().unwrap_or("".to_string()),
            )
        } else {
            self.label.clone()
        };

        let bottom_left = if self.size > 1_073_741_824 {
            Some(format!(
                "{:.2} GB",
                (self.size as f64 / 1024.0 / 1024.0 / 1024.0)
            ))
        } else if self.size > 0 {
            Some(format!("{:.1} MB", (self.size as f64 / 1024.0 / 1024.0)))
        } else if let Some(rating) = self.rating {
            // There's likely a better approach to parsing this.
            let pos = self.file.chars().rev().position(|c| c == '/');
            let filename = if let Some(pos) = pos {
                let position = self.file.len() - pos;
                &self.file[position..]
            } else {
                ""
            };

            if rating > 0.0 {
                Some(format!("Rating: {:.1} - {}", rating, filename))
            } else {
                if !filename.is_empty() {
                    Some(filename.to_string())
                } else {
                    None
                }
            }
        } else {
            None
        };

        let bottom_right = if self.type_ == VideoType::Movie {
            Some(format!("{}", self.year.unwrap()))
        } else {
            Some(self.lastmodified.clone())
        };

        crate::ListData {
            label,
            on_click: crate::Message::KodiReq(match self.filetype.as_str() {
                "directory" => KodiCommand::GetDirectory {
                    path: self.file.clone(),
                    media_type: MediaType::Video,
                },
                "file" => KodiCommand::PlayerOpen(self.file.clone()),
                _ => panic!("Impossible kodi filetype {}", self.filetype),
            }),
            play_count: self.playcount,
            bottom_right,
            bottom_left,
            image: Arc::new(OnceLock::new()),
        }
    }

    fn get_art_data(&self, http_url: &String) -> Pic {
        if self.type_ == VideoType::Episode && self.art.thumb.is_some() {
            let thumb = self.art.thumb.as_ref().unwrap();
            let thumb = urlencoding::encode(thumb.as_str());
            Pic {
                url: format!("{}/image/{}", http_url, thumb),
                w: 192,
                h: 108,
            }
        } else if self.art.poster.is_some() {
            let poster = self.art.poster.as_ref().unwrap();
            let poster = urlencoding::encode(poster.as_str());
            Pic {
                url: format!("{}/image/{}", http_url, poster),
                w: 80,
                h: 120,
            }
        } else {
            let icon = if self.filetype.eq_ignore_ascii_case("directory") {
                urlencoding::encode("image://DefaultFolder.png/")
            } else {
                urlencoding::encode("image://DefaultFile.png/")
            };
            Pic {
                url: format!("{}/image/{}", http_url, icon),
                h: 120,
                w: 80,
            }
        }
    }

    fn label_contains(&self, find: &String) -> bool {
        self.label.to_lowercase().contains(&find.to_lowercase())
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum VideoType {
    Episode,
    Movie,
    TVShow,
    #[default]
    Unknown,
}

// I'm not sure these need to be Option<>?
// They just return blank / DefaultVideo.png otherwise.
#[derive(Deserialize, Debug, Clone)]
pub struct Art {
    pub thumb: Option<String>,
    pub poster: Option<String>,
    // fanart: String,
    // landscape: Option<String>,
    // clearlogo: Option<String>,
    // icon: String (never used)
}

pub const PLAYING_ITEM_PROPS: [&'static str; 28] = [
    "album",
    "albumartist",
    "artist",
    "episode",
    "art",
    "file",
    "genre",
    "plot",
    "rating",
    "season",
    "showtitle",
    "studio",
    "tagline",
    "title",
    "track",
    "year",
    "streamdetails",
    "originaltitle",
    "playcount",
    "runtime",
    "duration",
    "cast",
    "writer",
    "director",
    "userrating",
    "firstaired",
    "displayartist",
    "uniqueid",
];
// TODO: LOTS more info
// Might be ListItem that's returned by playingitem?
// note a lot of this stuff is likely reutrned blank/default instead of Option
// I might make this very minimal then
// dispatch deserialization to MoveProps/EpisodeProps based minimal ver
#[derive(Deserialize, Debug, Clone, Default)]
pub struct PlayingItem {
    pub label: String,
    pub title: String,
    // album: String,
    // artist: Struct // TODO!
    // track: i16,
    // cast: Vec<Cast>
    // director: Vec<String>
    pub file: String,
    // firstaired: String, //Could convert this to date myself?
    // playcount: u8,
    // plot: String,
    // rating: f64,
    // runtime: u32, // useless for currently playing item. Might be used for ListItem?
    // streamdetails: StreamDetails,
    // studio: Struct // TODO!
    // tagline: String,
    // writer: Struct // TODO!
    // usually these items have a default, but some video streams just leave them out entirely
    pub year: Option<u16>,
    pub showtitle: Option<String>,
    pub episode: Option<i16>,
    pub season: Option<i16>,

    // id: Option<i16> // ???
    #[serde(rename = "type")]
    pub type_: VideoType,
    // there's also ignored field 'userrating' but I think it's useless.
}

impl PlayingItem {
    // this might change once the full PlayingItem is stored in main
    pub fn make_title(self) -> String {
        if self.type_ == VideoType::Episode {
            format!(
                "{} - S{:02}E{:02} - {}",
                self.showtitle.unwrap_or("".to_string()),
                self.season.unwrap_or(0),
                self.episode.unwrap_or(0),
                self.title,
            )
        } else if self.type_ == VideoType::Movie {
            format!("{} ({})", self.title, self.year.unwrap_or(0),)
        } else {
            self.label
        }
    }
}

// pub const DETAILED_MOVIE_PROPS: [&'static str; 25] = [
//     "title",
//     "genre",
//     "year",
//     "rating",
//     "director",
//     "trailer",
//     "tagline",
//     "plot",
//     "originaltitle",
//     "lastplayed",
//     "playcount",
//     "writer",
//     // "studio",
//     "mpaa",
//     "cast",
//     "country",
//     // "imdbnumber",
//     "runtime",
//     // "set",
//     "streamdetails",
//     // "votes",
//     "file",
//     // "sorttitle",
//     "resume",
//     "setid",
//     "dateadded",
//     "tag",
//     "art",
//     "premiered",
//     "uniqueid",
// ];

pub const MINIMAL_TV_PROPS: [&'static str; 10] = [
    "title",
    "year",
    "file",    // just returns the folder, not sure I even need this.
    "season",  //no of seasons
    "episode", // count of them. watchedepisodes also a thing
    "dateadded",
    "genre",
    "rating",
    // "premiered",
    "playcount",
    "art",
    // sorttitle //? might be useless?
];

pub const MINIMAL_EP_PROPS: [&'static str; 12] = [
    "title",
    "tvshowid",
    "file",
    "season",  // season no
    "episode", // ep no
    "dateadded",
    "rating",
    "firstaired",
    "playcount",
    "art",
    "specialsortseason",
    "specialsortepisode",
    // "showtitle", // ?? not sure if I want/need
];

#[derive(Deserialize, Debug, Clone)]
pub struct TVShowListItem {
    pub tvshowid: u32,
    pub title: String,
    pub year: u16,
    pub season: u16,
    pub episode: u16,
    pub file: String,
    pub dateadded: String,
    pub genre: Vec<String>,
    pub rating: f64,
    // pub premiered: String,
    pub playcount: i16,
    pub art: Art,
}

pub const TV_SEASON_PROPS: [&'static str; 4] = ["tvshowid", "title", "season", "episode"];

#[derive(Deserialize, Debug, Clone)]
pub struct TVSeasonListItem {
    pub seasonid: u32,
    pub tvshowid: u32,
    pub title: String,
    // i16 so I can set to -1 for "all seasons"
    pub season: i16,
    pub episode: u16,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TVEpisodeListItem {
    pub episodeid: u32,
    pub tvshowid: u32,
    pub title: String,
    pub season: u16,
    pub episode: u16,
    pub file: String,
    pub dateadded: String,
    pub rating: f64,
    pub firstaired: String,
    pub playcount: i16,
    pub art: Art,
    pub specialsortseason: i16, // annoyingly these are -1 for non-special
    pub specialsortepisode: i16,
    // showtitle? - really nly for the breadcrumb thing
}

impl IntoListData for TVShowListItem {
    fn into_listdata(&self) -> crate::ListData {
        // This is where it gets complicated.
        // I need to do a DBReq here to make it show the seasons - supply tvshow
        let on_click = crate::Message::DbQuery(SqlCommand::GetTVSeasons(self.clone()));

        let bottom_left = Some(format!("Rating: {:.1}", self.rating));
        crate::ListData {
            label: self.title.clone(),
            on_click,
            play_count: Some(self.playcount),
            bottom_left,
            bottom_right: Some(self.genre.join(", ")),
            image: Arc::new(OnceLock::new()),
        }
    }

    fn get_art_data(&self, http_url: &String) -> Pic {
        if self.art.poster.is_some() {
            let poster = self.art.poster.as_ref().unwrap();
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

    fn label_contains(&self, find: &String) -> bool {
        // Can also search originaltitle etc with this.
        self.title.to_lowercase().contains(&find.to_lowercase())
    }
}

impl IntoListData for TVSeasonListItem {
    fn into_listdata(&self) -> crate::ListData {
        // This is where it gets complicated.
        // I need to do a DBReq here to make it show the episodes with the tvshowid, season

        let on_click =
            crate::Message::DbQuery(SqlCommand::GetTVEpisodes(self.tvshowid, self.season));

        crate::ListData {
            label: self.title.clone(),
            on_click,
            play_count: None,
            bottom_left: None,
            bottom_right: Some(format!("{} Episodes", self.episode)),
            image: Arc::new(OnceLock::new()),
        }
    }

    fn get_art_data(&self, http_url: &String) -> Pic {
        let poster = urlencoding::encode("image://DefaultFolder.png/");
        Pic {
            url: format!("{}/image/{}", http_url, poster),
            w: 80,
            h: 120,
        }
    }

    fn label_contains(&self, find: &String) -> bool {
        // Can also search originaltitle etc with this.
        self.title.to_lowercase().contains(&find.to_lowercase())
    }
}

impl IntoListData for TVEpisodeListItem {
    fn into_listdata(&self) -> crate::ListData {
        let on_click = crate::Message::KodiReq(KodiCommand::PlayerOpen(self.file.clone()));

        // There's likely a better approach to parsing this.
        let pos = self.file.chars().rev().position(|c| c == '/');
        let filename = if let Some(pos) = pos {
            let position = self.file.len() - pos;
            &self.file[position..]
        } else {
            ""
        };
        let bottom_left = Some(format!("Rating: {:.1} - {}", self.rating, filename));

        let label = format!("S{:02}E{:02} - {}", self.season, self.episode, self.title,);

        crate::ListData {
            label,
            on_click,
            play_count: Some(self.playcount),
            bottom_left,
            bottom_right: Some(self.firstaired.clone()),
            image: Arc::new(OnceLock::new()),
        }
    }

    fn get_art_data(&self, http_url: &String) -> Pic {
        if self.art.thumb.is_some() {
            let thumb = self.art.thumb.as_ref().unwrap();
            let thumb = urlencoding::encode(thumb.as_str());
            Pic {
                url: format!("{}/image/{}", http_url, thumb),
                w: 192,
                h: 108,
            }
        } else {
            Pic {
                url: "".to_string(),
                h: 0,
                w: 0,
            }
        }
    }

    fn label_contains(&self, find: &String) -> bool {
        // Can also search originaltitle etc with this.
        self.title.to_lowercase().contains(&find.to_lowercase())
    }
}
// should add originaltitle for searching, and resume?
//   runtime might also be nice for list display
pub const MINIMAL_MOVIE_PROPS: [&'static str; 9] = [
    "title",
    "year",
    "file",
    "dateadded",
    "genre",
    "rating",
    "premiered",
    "playcount",
    "art",
];

#[derive(Deserialize, Debug, Clone)]
pub struct MovieListItem {
    pub movieid: u32,
    pub title: String,
    pub year: u16,
    pub file: String,
    pub dateadded: String,
    pub genre: Vec<String>,
    pub rating: f64,
    pub premiered: String,
    pub playcount: i16,
    pub art: Art,
}

impl IntoListData for MovieListItem {
    fn into_listdata(&self) -> crate::ListData {
        let on_click = crate::Message::KodiReq(KodiCommand::PlayerOpen(self.file.clone()));

        // There's likely a better approach to parsing this.
        let pos = self.file.chars().rev().position(|c| c == '/');
        let filename = if let Some(pos) = pos {
            let position = self.file.len() - pos;
            &self.file[position..]
        } else {
            ""
        };

        let bottom_left = Some(format!("Rating: {:.1} - {}", self.rating, filename));
        crate::ListData {
            label: self.title.clone(),
            on_click,
            play_count: Some(self.playcount),
            bottom_left,
            bottom_right: Some(self.year.to_string()),
            image: Arc::new(OnceLock::new()),
        }
    }

    fn get_art_data(&self, http_url: &String) -> Pic {
        if self.art.poster.is_some() {
            let poster = self.art.poster.as_ref().unwrap();
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

    fn label_contains(&self, find: &String) -> bool {
        // Can also search originaltitle etc with this.
        self.title.to_lowercase().contains(&find.to_lowercase())
    }
}

// #[derive(Deserialize, Debug, Clone)]
// pub struct MovieProps {
//     pub movieid: u32,
//     pub title: String,
//     pub genre: Vec<String>,
//     pub year: u16,
//     pub rating: f64,
//     pub director: Vec<String>,
//     pub trailer: String,
//     pub tagline: String,
//     pub plot: String,
//     pub originaltitle: String,
//     pub lastplayed: String, // maybe date?
//     pub playcount: u16,
//     pub writer: Vec<String>,
//     pub mpaa: String,
//     pub cast: Vec<Cast>,
//     pub country: Vec<String>,
//     pub runtime: u32,
//     pub streamdetails: StreamDetails,
//     // votes: String,
//     pub file: String,
//     pub resume: ResumePoint,
//     pub setid: u16,
//     pub dateadded: String,
//     pub tag: Vec<String>,
//     pub art: Art,
//     pub premiered: String,
//     pub uniqueid: UniqueId,
// }

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Cast {
    name: String,
    order: u16,
    role: String,
    thumbnail: Option<String>,
}

// #[derive(Deserialize, Debug, Clone)]
// pub struct UniqueId {
//     pub imdb: String,
// }

#[derive(Deserialize, Debug, Clone, Default)]
pub struct ResumePoint {
    pub position: f64, // Position of resume in seconds
    pub total: f64,    // total runtime again for some reason
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct StreamDetails {
    audio: Vec<ItemAudio>,
    subtitle: Vec<ItemSubtitle>,
    video: Vec<ItemVideo>,
}
// very similar to AudioStream but with less fields
// Seemed easier to make a new type than option a bunch of stuff
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ItemAudio {
    channels: u8,
    codec: String,
    language: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ItemSubtitle {
    language: String,
}

// unlike the last 2 this has a bit more/different info than VideoStream
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ItemVideo {
    aspect: f64,
    codec: String,
    duration: u32,
    hdrtype: String,
    height: u16,
    width: u16,
    language: String,
    stereomode: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KodiServer {
    pub id: u8,
    pub name: String,
    pub ip: String,
    pub websocket_port: u16,
    pub webserver_port: u16,
    pub username: String,
    pub password: String,
    pub db_id: u8, // The movie/tv info database number for sharing
                   // KodiServer.id==1 can use the same db_id=0 as id==0
}

impl KodiServer {
    pub fn new(
        name: String,
        ip: String,
        websocket_port: u16,
        webserver_port: u16,
        username: String,
        password: String,
    ) -> Self {
        KodiServer {
            id: 0,
            name,
            ip,
            websocket_port,
            webserver_port,
            username,
            password,
            db_id: 0,
        }
    }

    pub fn websocket_url(&self) -> String {
        format!("ws://{}:{}", self.ip, self.websocket_port)
    }
    pub fn http_url(&self) -> String {
        format!("http://{}:{}", self.ip, self.webserver_port)
    }
}
