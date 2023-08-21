use iced::widget::button;
use iced::{Background, Color};

const DARK: Color = Color::from_rgb(0.2, 0.2, 0.2);
const DARK_HILIGHT: Color = Color::from_rgb(0.3, 0.3, 0.3);

#[derive(Debug, Clone, Copy)]
pub enum ColoredButton {
    ListItem,
    Bare,
}

impl button::StyleSheet for ColoredButton {
    type Style = iced::Theme;

    fn active(&self, _style: &Self::Style) -> button::Appearance {
        match self {
            ColoredButton::ListItem => button::Appearance {
                background: Some(Background::Color(DARK)),
                text_color: Color::WHITE,
                border_radius: 3.0.into(),
                ..Default::default()
            },
            ColoredButton::Bare => button::Appearance {
                background: Some(Background::Color(Color::TRANSPARENT)),
                text_color: Color::WHITE,
                border_radius: 3.0.into(),
                ..Default::default()
            },
        }
    }

    fn pressed(&self, style: &Self::Style) -> button::Appearance {
        let active = self.active(style);
        button::Appearance { ..active }
    }

    fn hovered(&self, _style: &Self::Style) -> button::Appearance {
        // let active = self.active(style);
        button::Appearance {
            background: Some(Background::Color(DARK_HILIGHT)),
            text_color: Color::WHITE,
            border_radius: 3.0.into(),
            ..Default::default()
        }
    }

    fn disabled(&self, style: &Self::Style) -> button::Appearance {
        let active = self.active(style);

        button::Appearance {
            text_color: Color {
                a: 0.2,
                ..active.text_color
            },
            border_color: Color {
                a: 0.2,
                ..active.border_color
            },
            ..active
        }
    }
}
