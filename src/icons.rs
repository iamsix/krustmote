#![allow(dead_code)]
use iced::Font;
use iced::widget::{text, Text};
use iced::alignment;


const ICONS: Font = Font {
    monospaced: true,
    ..Font::with_name("Material Icons") 
};

pub fn folder() -> Text<'static> {icon('\u{e2c7}')}
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

fn icon(unicode: char) -> Text<'static> {
    
    
    text(unicode.to_string())
        .font(ICONS)
    //    .width(20) // Width vs size??
        .horizontal_alignment(alignment::Horizontal::Center)
        .vertical_alignment(alignment::Vertical::Center)
     //   .size(20) // Not sure this should be here.
}