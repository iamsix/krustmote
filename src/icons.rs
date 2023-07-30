use iced::Font;
use iced::widget::{text, Text};
use iced::alignment;


const ICONS: Font = Font::External { 
    name: "Icons", 
    bytes: include_bytes!("../fonts/MaterialIcons-Regular.ttf"),
};

pub fn folder() -> Text<'static> {icon('\u{e2c7}')}
pub fn sync() -> Text<'static> {icon('\u{e627}')}
pub fn sync_disabled() -> Text<'static> {icon('\u{e628}')}

fn icon(unicode: char) -> Text<'static> {
    text(unicode.to_string())
        .font(ICONS)
        .width(20)
        .horizontal_alignment(alignment::Horizontal::Center)
        .size(20)
}