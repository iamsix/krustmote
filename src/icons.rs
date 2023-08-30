#![allow(dead_code)]
use iced::Font;
use iced::widget::{text, Text};
use iced::alignment;


const ICONS: Font = Font {
    monospaced: true,
    ..Font::with_name("Material Icons") 
};

pub fn folder() -> Text<'static> {icon('\u{e2c7}')}
pub fn settings() -> Text<'static> {icon('\u{e8b8}')}
pub fn sync() -> Text<'static> {icon('\u{e627}')}
pub fn sync_disabled() -> Text<'static> {icon('\u{e628}')}

pub fn bug_report() -> Text<'static> {icon('\u{e868}')}

pub fn volume_down() -> Text<'static> {icon('\u{e04d}')}
pub fn volume_up() -> Text<'static> {icon('\u{e050}')}
pub fn volume_off() -> Text<'static> {icon('\u{e04f}')}

pub fn fullscreen() -> Text<'static> {icon('\u{e5d0}')}
pub fn info() -> Text<'static> {icon('\u{e88e}')}
pub fn keyboard() -> Text<'static> {icon('\u{e312}')}

pub fn call_to_action() -> Text<'static> {icon('\u{e06c}')}
pub fn format_list_bulleted() -> Text<'static> {icon('\u{e241}')}

pub fn expand_less() -> Text<'static> {icon('\u{e5ce}')}
pub fn chevron_left() -> Text<'static> {icon('\u{e5cb}')}
pub fn chevron_right() -> Text<'static> {icon('\u{e5cc}')}
pub fn expand_more() -> Text<'static> {icon('\u{e5cf}')}
pub fn circle() -> Text<'static> {icon('\u{ef4a}')}
pub fn arrow_back() -> Text<'static> {icon('\u{e5c4}')}

pub fn done() -> Text<'static> {icon('\u{e876}')}

pub fn pause_clircle_filled() -> Text<'static> {icon('\u{e035}')}
pub fn play_circle_filled() -> Text<'static> {icon('\u{e038}')}
pub fn stop() -> Text<'static> {icon('\u{e047}')}
pub fn fast_rewind() -> Text<'static> {icon('\u{e020}')}
pub fn fast_forward() -> Text<'static> {icon('\u{e01f}')}
pub fn skip_previous() -> Text<'static> {icon('\u{e045}')}
pub fn skip_next() -> Text<'static> {icon('\u{e044}')}

pub fn subtitles() -> Text<'static> {icon('\u{e048}')}
pub fn smart_display() -> Text<'static> {icon('\u{f06a}')}
pub fn hearing() -> Text<'static> {icon('\u{e023}')}

fn icon(unicode: char) -> Text<'static> {
    
    
    text(unicode.to_string())
        .font(ICONS)
        .horizontal_alignment(alignment::Horizontal::Center)
        .vertical_alignment(alignment::Vertical::Center)
        .line_height(1.0)
  }