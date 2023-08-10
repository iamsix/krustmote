use serde::{Serialize, Deserialize};

#[derive(Deserialize, Clone, Debug)]
pub struct PlayerProps {
    pub speed: f64,
    pub time: KodiTime,
    pub totaltime: KodiTime,
    pub player_id: Option<u8>,
    // currentaudiostream: AudioStream,
    // audiostreams: Vec[AudioStream],
    pub canseek: bool,
    // pub currentsubtitle: Subtitle,
    // pub subtitles: Vec<Subtitle>,
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

// #[derive(Deserialize, Clone, Debug, Default)]
// pub struct VideoStream {
//     codec: String,
//     height: u16,
//     index: u8,
//     language: String,
//     name: String,
//     width: u16,
// }

// #[derive(Deserialize, Clone, Debug)]
// pub struct Subtitle {
//     index: u8,
//     isdefault: bool,
//     isforced: bool,
//     isimpaired: bool,
//     language: String,
//     name: String,
// }


#[derive(Deserialize, Clone, Debug, Default)]
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
        write!(f, "{:02}:{:02}:{:02}", self.hours, self.minutes, self.seconds)
    }
}



#[derive(Deserialize, Debug, Clone)]
pub struct KodiAppStatus {
    pub muted: bool,
    //volume: u8,
}



#[derive(Debug, Clone)]
pub enum KodiCommand {
    Test,
    GetSources(MediaType), // TODO: SortType
    GetDirectory{path: String, media_type: MediaType}, // TODO: SortType
    PlayerOpen(String),
    InputButtonEvent{button: &'static str, keymap: &'static str},
    InputExecuteAction(&'static str),
    // ToggleMute,
    // PlayerPlayPause,
    // PlayerStop,
    // GUIActivateWindow(String),

    // Not sure if I actually need these ones from the front end. (they're used by back end)
     PlayerGetProperties, // Possibly some variant of this one to get subs/audio/video
     PlayerGetPlayingItem(u8),
     PlayerGetActivePlayers, 
}


#[derive(Deserialize, Debug, Clone, Copy)]
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
    pub filetype: String,
    pub label: String,
    pub showtitle: Option<String>,
    pub title: Option<String>,
    pub lastmodified: String,
    pub size: u64,
    pub playcount: Option<u16>,
    #[serde(rename = "type")]
    pub type_: String, // Should be enum from string
}

// TODO: LOTS more info
// Might be ListItem that's returned by playingitem?
#[derive(Deserialize, Debug, Clone, Default)]
pub struct PlayingItem {
    pub label: String,
    pub title: String,
    // album: String,
    // artist: Struct // TODO!
    // track: i16,
    // cast: Struct // TODO!
    // director: Struct // TODO!
    // file: String,
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
    // showtitle: String,
    // episode: i16,
    // season: i16,

    // id: Option<i16> // ???

    // this is the "episode" "movie" etc type - not filetype/MediaType
    // type_: String, // Should be enum from string 
    // there's also ignored field 'userrating' but I think it's useless.
}
