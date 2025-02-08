use iced::widget::button;
use iced::Color;
use iced::Theme;

const DARK: Color = Color::from_rgb(0.2, 0.2, 0.2);
const DARK_HILIGHT: Color = Color::from_rgb(0.3, 0.3, 0.3);

pub fn bare_button(_theme: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Active => button::Style {
            background: None,
            text_color: Color::WHITE,
            ..Default::default()
        },
        button::Status::Hovered => button::Style {
            background: Some(iced::Background::Color(DARK_HILIGHT)),
            text_color: Color::WHITE,
            ..Default::default()
        },
        button::Status::Pressed => button::Style {
            background: Some(iced::Background::Color(DARK)),
            text_color: Color::WHITE,
            ..Default::default()
        },
        button::Status::Disabled => button::Style {
            background: None,
            text_color: Color::from_rgba(1.0, 1.0, 1.0, 0.2),
            ..Default::default()
        },
    }
}

pub fn listitem(_theme: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Active => button::Style {
            background: Some(iced::Background::Color(DARK)),
            text_color: Color::WHITE,
            ..Default::default()
        },
        button::Status::Hovered => button::Style {
            background: Some(iced::Background::Color(DARK_HILIGHT)),
            text_color: Color::WHITE,
            ..Default::default()
        },
        button::Status::Pressed => button::Style {
            background: Some(iced::Background::Color(DARK)),
            text_color: Color::WHITE,
            ..Default::default()
        },
        button::Status::Disabled => button::Style {
            background: Some(iced::Background::Color(DARK)),
            text_color: Color::from_rgba(1.0, 1.0, 1.0, 0.2),
            ..Default::default()
        },
    }
}
