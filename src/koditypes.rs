use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::sync::{Arc, OnceLock};

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

fn treat_error_as_none<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;
    Ok(T::deserialize(value).ok())
}

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

#[derive(Debug, Clone, PartialEq)]
pub enum KodiCommand {
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
    InputSendText(String),

    PlayerGetProperties,
    PlayerGetPlayingItem(u8),
    PlayerGetActivePlayers,

    // only used for testing/debug:
    PlayerGetPlayingItemDebug(u8),
    Test,
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

impl Into<crate::ListData> for Sources {
    fn into(self) -> crate::ListData {
        crate::ListData {
            label: self.label,
            on_click: crate::Message::KodiReq(KodiCommand::GetDirectory {
                path: self.file,
                media_type: MediaType::Video,
            }),
            play_count: None,
            bottom_right: None,
            bottom_left: None,
            image: Arc::new(OnceLock::new()),
        }
    }
}

// TODO: SortType that defines these
#[derive(Serialize, Debug)]
pub struct DirSort {
    pub method: &'static str,
    pub order: &'static str,
}

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
    pub playcount: Option<u16>,
    pub year: Option<u16>,
    #[serde(rename = "type")]
    pub type_: VideoType, // Should be enum from string
}

// NOTE: this leaves the image blank for now.
// Could probably fix that by doing Into<Vec<ListData> for Vec<DirList>
impl Into<crate::ListData> for DirList {
    fn into(self) -> crate::ListData {
        let label = if self.type_ == VideoType::Episode {
            format!(
                "{} - S{:02}E{:02} - {}",
                self.showtitle.unwrap_or("".to_string()),
                self.season.unwrap_or(0),
                self.episode.unwrap_or(0),
                self.title.unwrap_or("".to_string()),
            )
        } else {
            self.label
        };

        let bottom_left = if self.size > 1_073_741_824 {
            Some(format!(
                "{:.2} GB",
                (self.size as f64 / 1024.0 / 1024.0 / 1024.0)
            ))
        } else if self.size > 0 {
            Some(format!("{:.1} MB", (self.size as f64 / 1024.0 / 1024.0)))
        } else if let Some(rating) = self.rating {
            if rating > 0.0 {
                Some(format!("Rating: {:.1}", rating))
            } else {
                None
            }
        } else {
            None
        };

        let bottom_right = if self.type_ == VideoType::Movie {
            Some(format!("{}", self.year.unwrap()))
        } else {
            Some(self.lastmodified)
        };

        crate::ListData {
            label,
            on_click: crate::Message::KodiReq(match self.filetype.as_str() {
                "directory" => KodiCommand::GetDirectory {
                    path: self.file,
                    media_type: MediaType::Video,
                },
                "file" => KodiCommand::PlayerOpen(self.file),
                _ => panic!("Impossible kodi filetype {}", self.filetype),
            }),
            play_count: self.playcount,
            bottom_right,
            bottom_left,
            image: Arc::new(OnceLock::new()),
        }
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

#[derive(Deserialize, Debug, Clone)]
pub struct Art {
    pub thumb: Option<String>,
    pub poster: Option<String>,
}

// TODO: LOTS more info
// Might be ListItem that's returned by playingitem?
// note a lot of this stuff is likely reutrned blank/default instead of Option
#[derive(Deserialize, Debug, Clone, Default)]
pub struct PlayingItem {
    pub label: String,
    pub title: String,
    // album: String,
    // artist: Struct // TODO!
    // track: i16,
    // cast: Struct // TODO!
    // director: Struct // TODO!
    pub file: String,
    // firstaired: String, //Could convert this to date myself?
    // playcount: u8,
    // plot: String,
    // rating: f64,
    // runtime: u32, // useless for currently playing item. Might be used for ListItem?
    // streamdetails: TODO! struct: audio<vec> video<vec> subtitle<vec>
    //                Note they're not quite the same as the PlayerProps
    //                models but somewhat similar
    // studio: Struct // TODO!
    // tagline: String,
    // writer: Struct // TODO!
    // year: u16,
    // These might not need to be Options - it seems to always return some default
    pub showtitle: Option<String>,
    pub episode: Option<i16>,
    pub season: Option<i16>,

    // id: Option<i16> // ???
    #[serde(rename = "type")]
    pub type_: VideoType,
    // there's also ignored field 'userrating' but I think it's useless.
}
