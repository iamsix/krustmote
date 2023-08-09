use serde::{Serialize, Deserialize};

#[derive(Deserialize, Clone, Debug)]
pub struct PlayerProps {
    pub speed: f64,
    pub time: KodiTime,
    pub totaltime: KodiTime,
    // currentaudiostream: AudioStream,
    // audiostreams: Vec[AudioStream],
    // canseek: bool,
    // currentsubtitle: Subtitle,
    // subtitles: Vec[Subtitles]
    // currentvideostream: VideoStream,
    // videostreams: Vec[VideoStream],
    // playlistid: u8,
    // position: u8,
    // repeat: String //(could be enum?)
    // shuffled: bool,
    // subtitleenabled: bool,
    // type_: MediaType // need impl fromstring

}


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
     PlayerGetPlayingItem,
     PlayerGetActivePlayers, 
}

#[derive(Debug, Clone, Copy)]
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
  //  playertype: String,
  //  type_: MediaType //need to impl 'from' string on that.
}


// TODO: proper serde models for all the useful outputs
// Likely need a whole file just to contain them
// Almost need a file of various enums/structs/etc anyway...
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
    pub lastmodified: String,
    pub size: u64,
    pub playcount: Option<u16>,
    #[serde(rename = "type")]
    pub type_: String, // Should be enum from string
}
