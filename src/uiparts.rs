use iced::Color;
use iced::Element;
use iced::Length;

use iced::font::{Family, Font, Weight};
use iced::widget::scrollable::Id;
use iced::widget::{
    button, column, container, image, pick_list, row, scrollable, text, text_input, Button,
    Checkbox, Rule, Slider, Space,
};

use super::Krustmote;
use super::BLANK_IMAGE;
use super::ITEM_HEIGHT;
use super::{ListData, Message, Modals, State};

use crate::db;
use crate::icons;
use crate::koditypes::*;
use crate::themes;

use chrono;

pub(crate) fn make_subtitle_modal<'a>(
    krustmote: &'a Krustmote,
) -> iced::widget::Container<'a, Message> {
    container(column![
        row![
            text("Subtitles").height(40),
            Space::new(Length::Fill, 10),
            // This is likely the only place this Message is used
            // however it's the only way I can think to do this
            button("Download").on_press(Message::HideModalAndKodiReq(
                KodiCommand::GUIActivateWindow("subtitlesearch")
            )),
        ],
        Rule::horizontal(5),
        row![
            pick_list(
                &*krustmote.kodi_status.player_props.subtitles,
                krustmote.kodi_status.player_props.currentsubtitle.clone(),
                Message::SubtitlePicked
            )
            .placeholder("No Subtitles")
            .width(Length::Fill),
            Checkbox::new("", krustmote.kodi_status.player_props.subtitleenabled)
                .on_toggle(Message::SubtitleToggle),
        ],
        row![
            button("-").on_press(Message::KodiReq(KodiCommand::InputExecuteAction(
                "subtitledelayminus"
            ))),
            text(" Delay "),
            button("+").on_press(Message::KodiReq(KodiCommand::InputExecuteAction(
                "subtitledelayplus"
            )))
        ]
        .align_y(iced::Alignment::Center),
        // Subtitle adjust buttons.
    ])
    .width(500)
    .padding(10)
    .style(|_| container::Style::default().background(iced::Theme::Dracula.palette().background))
}

pub(crate) fn make_audio_modal<'a>(
    krustmote: &'a Krustmote,
) -> iced::widget::Container<'a, Message> {
    container(column![
        row![
            text("Audio").height(40),
            Space::new(Length::Fill, 10),
            button("x").on_press(Message::ShowModal(crate::Modals::None)),
        ],
        Rule::horizontal(5),
        pick_list(
            &*krustmote.kodi_status.player_props.audiostreams,
            krustmote
                .kodi_status
                .player_props
                .currentaudiostream
                .clone(),
            Message::AudioStreamPicked
        )
        .placeholder("No Audio")
        .width(Length::Fill),
        row![
            button("-").on_press(Message::KodiReq(KodiCommand::InputExecuteAction(
                "audiodelayminus"
            ))),
            text(" Delay "),
            button("+").on_press(Message::KodiReq(KodiCommand::InputExecuteAction(
                "audiodelayplus"
            )))
        ]
        .align_y(iced::Alignment::Center),
    ])
    .width(500)
    .padding(10)
    .style(|_| container::Style::default().background(iced::Theme::Dracula.palette().background))
}

pub(crate) fn request_text_modal<'a>(
    krustmote: &'a Krustmote,
) -> iced::widget::Container<'a, Message> {
    container(column![
        text_input("Text to send...", &krustmote.send_text)
            .on_input(Message::SendTextInput)
            .on_submit(Message::HideModalAndKodiReq(KodiCommand::InputSendText(
                krustmote.send_text.clone()
            ))),
        row![
            Space::new(Length::Fill, 5),
            button("Send").on_press(Message::HideModalAndKodiReq(KodiCommand::InputSendText(
                krustmote.send_text.clone()
            ))),
        ]
    ])
    .width(500)
    .padding(10)
    .style(|_| container::Style::default().background(iced::Theme::Dracula.palette().background))
}

pub(crate) fn playing_bar<'a>(krustmote: &'a Krustmote) -> Element<'a, Message> {
    let duration = krustmote.kodi_status.player_props.totaltime.total_seconds();
    let play_time = krustmote.kodi_status.player_props.time.total_seconds();
    let timeleft = duration.saturating_sub(play_time);
    let now = chrono::offset::Local::now();
    let end = now + chrono::Duration::seconds(timeleft as i64);
    let end = end.format("%I:%M %p");
    if krustmote.kodi_status.active_player_id.is_some() {
        container(
            row![
                Space::new(5, 5),
                column![
                    Slider::new(0..=duration, play_time, Message::SliderChanged)
                        .on_release(Message::SliderReleased),
                    row![
                        text(format!("{}", krustmote.kodi_status.player_props.time,)).size(14),
                        Space::new(Length::Fill, 5),
                        text(format!(
                            "{} ({end})",
                            krustmote.kodi_status.player_props.totaltime
                        ))
                        .size(14),
                    ],
                    text(&krustmote.kodi_status.playing_title)
                        .font(Font {
                            family: Family::SansSerif,
                            weight: Weight::Bold,
                            ..Default::default()
                        })
                        .shaping(text::Shaping::Advanced)
                        .wrapping(text::Wrapping::None)
                        .height(20),
                ]
                .width(Length::FillPortion(55)),
                row![
                    Space::new(Length::Fill, 5),
                    button(icons::skip_previous().size(32).height(48))
                        .style(themes::bare_button)
                        .on_press(Message::KodiReq(KodiCommand::InputExecuteAction(
                            "skipprevious"
                        ))),
                    button(icons::fast_rewind().size(32).height(48))
                        .style(themes::bare_button)
                        .on_press(Message::KodiReq(KodiCommand::InputExecuteAction("rewind"))),
                    button(if krustmote.kodi_status.player_props.speed != 0.0 {
                        icons::pause_clircle_filled().size(48)
                    } else {
                        icons::play_circle_filled().size(48)
                    })
                    .on_press(Message::KodiReq(KodiCommand::InputExecuteAction(
                        "playpause"
                    )))
                    .style(themes::bare_button),
                    button(icons::fast_forward().size(32).height(48))
                        .style(themes::bare_button)
                        .on_press(Message::KodiReq(KodiCommand::InputExecuteAction(
                            "fastforward"
                        ))),
                    button(icons::skip_next().size(32).height(48))
                        .style(themes::bare_button)
                        .on_press(Message::KodiReq(KodiCommand::InputExecuteAction(
                            "skipnext"
                        ))),
                    button(icons::stop().size(32).height(48))
                        .on_press(Message::KodiReq(KodiCommand::InputExecuteAction("stop")))
                        .style(themes::bare_button),
                    Space::new(20, 5),
                    column![
                        button(icons::subtitles())
                            .on_press(Message::ShowModal(Modals::Subtitles))
                            .style(themes::bare_button),
                        button(icons::hearing())
                            .on_press(Message::ShowModal(Modals::Audio))
                            .style(themes::bare_button),
                        button(icons::videocam()).style(themes::bare_button),
                    ],
                    Space::new(10, 5),
                ]
                .width(Length::FillPortion(40))
                .align_y(iced::Alignment::Center)
            ]
            .spacing(20),
        )
        .height(80)
        .into()
    } else {
        container(Space::new(Length::Fill, 80)).into()
    }
}

pub(crate) fn top_bar<'a>(krustmote: &Krustmote) -> Element<'a, Message> {
    container(row![
        button(icons::menu())
            .on_press(Message::ToggleLeftMenu)
            .style(themes::bare_button),
        Space::new(Length::Fill, Length::Shrink),
        text_input("Filter..", &krustmote.item_list.filter)
            .on_input(Message::FilterFileList)
            .id(text_input::Id::new("Filter")),
        button(" x ").on_press(Message::FilterFileList("".to_string()))
    ])
    .into()
}

pub(crate) fn center_area<'a>(krustmote: &'a Krustmote) -> Element<'a, Message> {
    match krustmote.content_area {
        crate::ContentArea::Files => file_list(krustmote),
        crate::ContentArea::Loading => loading(krustmote),
        _ => container("").into(),
    }
}

pub(crate) fn loading<'a>(_krustmote: &'a Krustmote) -> Element<'a, Message> {
    // TODO: Spinner.
    container(text("...").size(48))
        .width(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .into()
}

pub(crate) fn file_list<'a>(krustmote: &'a Krustmote) -> Element<'a, Message> {
    let offset = krustmote.item_list.start_offset;

    let mut virtual_list: Vec<Element<'a, Message>> = Vec::new();

    let top_space = offset * ITEM_HEIGHT;
    virtual_list.push(Space::new(10, top_space as f32).into());

    let files = krustmote
        .item_list
        .virtual_list
        .iter()
        .map(|(_, d)| make_listitem(d))
        .map(Element::from);

    virtual_list.extend(files);

    let bottom_space = if !krustmote.item_list.filter.is_empty() {
        krustmote.item_list.filtered_count as u32 * ITEM_HEIGHT
    } else {
        krustmote.item_list.raw_data.len() as u32 * ITEM_HEIGHT
    }
    .saturating_sub(krustmote.item_list.visible_count * ITEM_HEIGHT)
    .saturating_sub(offset * ITEM_HEIGHT);

    virtual_list.push(Space::new(10, bottom_space as f32).into());

    // dbg!(virtual_list.len());

    let virtual_list = column(virtual_list);

    column![
        row![if krustmote.item_list.breadcrumb.len() > 1 {
            button(column![
                "..",
                text(&krustmote.item_list.list_title).size(10)
            ])
            .on_press(Message::UpBreadCrumb)
            .width(Length::Fill)
            .height(50)
            .style(themes::listitem)
        } else {
            button(text(&krustmote.item_list.list_title))
                .width(Length::Fill)
                .height(50)
                .style(themes::listitem)
        },]
        .spacing(1)
        .padding(iced::Padding {
            left: 5.0,
            top: 5.0,
            right: 0.0,
            bottom: 5.0
        }),
        scrollable(virtual_list.spacing(1).padding(iced::Padding {
            left: 5.0,
            top: 5.0,
            right: 5.0,
            bottom: 5.0
        }),)
        .on_scroll(Message::Scrolled)
        .id(Id::new("files"))
    ]
    .width(Length::Fill)
    .into()
}

pub(crate) fn make_listitem(data: &ListData) -> Button<Message> {
    // Let's stretch the definition of a 'button'
    // ___________________________________________________________
    // | picture |  Main Label Information                       |
    // | picture |  (smaller text) content (genre, runtime, etc) |
    // | picture |  bottom left                     bottom right |
    // -----------------------------------------------------------
    //
    // row![ picture, column! [ label,
    //                          content,
    //                          row! [bottom_left, space, bottom_right],
    //                         ]
    //     ]
    // It seems pretty clear I'll have to make some kind of custom
    //    RecyclerView type thing.
    //    The button captures any attempt to touch-scroll.
    //    and there's no 'fling' anyway
    //
    // TODO: I should specify label heights here to ensure no line wrapping/etc
    let image_data = data.image.get();
    Button::new(row![
        if let Some(img) = image_data {
            container(image(img.clone()).height(45))
        } else {
            // Could use a Space here instead
            //   but BLANK_IMAGE will eventually be PLACEHOLDER_IMAGE
            container(image(BLANK_IMAGE.get().unwrap().clone()).height(45))
        },
        // Watched will proabbly go in picture area - for now just this icon or not
        if data.play_count.unwrap_or(0) > 0 {
            icons::done()
        } else {
            text(" ")
        },
        column![
            text(data.label.as_str()).size(14).height(19),
            text("").size(10),
            row![
                match &data.bottom_left {
                    Some(d) => text(d.as_str()).size(10),
                    None => text(""),
                },
                Space::new(Length::Fill, Length::Shrink),
                match &data.bottom_right {
                    Some(d) => text(d.as_str()).size(10),
                    None => text(""),
                },
            ]
        ]
    ])
    .on_press(data.on_click.clone())
    .width(Length::Fill)
    .height(ITEM_HEIGHT as f32)
    .style(themes::listitem)
}

pub(crate) fn left_menu<'a>(krustmote: &'a Krustmote) -> Element<'a, Message> {
    container(
        column![
            row![
                match krustmote.state {
                    State::Disconnected => icons::sync_disabled(),
                    _ => icons::sync(),
                },
                match &krustmote.kodi_status.server {
                    Some(s) => text(&s.name).size(14),
                    None => text("Disconnected").size(14),
                }
            ]
            .align_y(iced::Alignment::Center),
            Rule::horizontal(20),
            if let crate::State::Connected(..) = krustmote.state {
                container(
                    button(row![icons::folder(), "Files"].align_y(iced::Alignment::Center))
                        .on_press(Message::KodiReq(KodiCommand::GetSources(MediaType::Video)))
                        .width(Length::Fill)
                        .style(themes::bare_button),
                )
                .width(Length::Fill)
            } else {
                container("")
            },
            button(row![icons::movie(), "Movies"].align_y(iced::Alignment::Center))
                .on_press(Message::DbQuery(db::SqlCommand::GetMovieList))
                .width(Length::Fill)
                .style(themes::bare_button),
            button(row![icons::tv(), "TV"].align_y(iced::Alignment::Center))
                .on_press(Message::DbQuery(db::SqlCommand::GetTVShowList))
                .width(Length::Fill)
                .style(themes::bare_button),
            button(row![icons::settings(), "Settings"].align_y(iced::Alignment::Center))
                .width(Length::Fill)
                .style(themes::bare_button)
                .on_press(Message::ShowSettings),
        ]
        .spacing(1)
        .padding(5)
        .width(105),
    )
    .max_width(krustmote.menu_width)
    .into()
}

pub(crate) fn remote<'a>(krustmote: &Krustmote) -> Element<'a, Message> {
    if let crate::State::Disconnected = krustmote.state {
        return container("").into();
    }
    let red = Color::from_rgb8(255, 0, 0);
    container(
        column![
            // seems like I could template these buttons in some way
            button(icons::bug_report()).on_press(Message::KodiReq(KodiCommand::Test)),
            button("Movies-test").on_press(Message::KodiReq(KodiCommand::VideoLibraryGetMovies)),
            button("TV-test").on_press(Message::KodiReq(KodiCommand::VideoLibraryGetTVShows)),
            button("Sason-test").on_press(Message::KodiReq(KodiCommand::VideoLibraryGetTVSeasons)),
            button("Eps-test").on_press(Message::KodiReq(KodiCommand::VideoLibraryGetTVEpisodes)),
            button("playerid-test").on_press(Message::KodiReq(KodiCommand::PlayerGetActivePlayers)),
            button("props-test").on_press(Message::KodiReq(KodiCommand::PlayerGetProperties)),
            button("item-test").on_press(Message::KodiReq(KodiCommand::PlayerGetPlayingItemDebug(
                krustmote.kodi_status.active_player_id.unwrap_or(0)
            ))),
            row![
                button(icons::volume_down().size(32)).on_press(Message::KodiReq(
                    KodiCommand::InputExecuteAction("volumedown")
                )),
                if krustmote.kodi_status.muted {
                    button(icons::volume_off().color(red).size(32))
                        .on_press(Message::KodiReq(KodiCommand::ToggleMute))
                } else {
                    button(icons::volume_off().size(32))
                        .on_press(Message::KodiReq(KodiCommand::ToggleMute))
                },
                button(icons::volume_up().size(32)).on_press(Message::KodiReq(
                    KodiCommand::InputExecuteAction("volumeup")
                )),
            ]
            .spacing(10),
            row![
                button(icons::fullscreen().size(30)).on_press(Message::KodiReq(
                    KodiCommand::InputButtonEvent {
                        button: "display",
                        keymap: "R1"
                    }
                )),
                button(icons::info().size(30)).on_press(Message::KodiReq(
                    KodiCommand::InputButtonEvent {
                        button: "info",
                        keymap: "R1"
                    }
                )),
                button(icons::keyboard().size(30))
                    .on_press(Message::ShowModal(Modals::RequestText)),
            ]
            .spacing(10),
            row![
                button(icons::call_to_action().size(30)).on_press(Message::KodiReq(
                    KodiCommand::InputButtonEvent {
                        button: "menu",
                        keymap: "R1"
                    }
                )),
                Space::new(40, 40), // Not sure what to put here.
                button(icons::format_list_bulleted().size(30)).on_press(Message::KodiReq(
                    KodiCommand::InputButtonEvent {
                        button: "title",
                        keymap: "R1"
                    }
                )),
            ]
            .spacing(10),
            Space::new(Length::Shrink, Length::Fill),
            row![
                // Might add pgup/pgdn buttons on either side here.
                Space::new(65, 65),
                button(icons::expand_less().size(48))
                    .width(65)
                    .height(65)
                    .on_press(Message::KodiReq(KodiCommand::InputButtonEvent {
                        button: "up",
                        keymap: "R1",
                    })),
                Space::new(65, 65),
            ]
            .spacing(5),
            row![
                button(icons::chevron_left().size(48))
                    .width(65)
                    .height(65)
                    .on_press(Message::KodiReq(KodiCommand::InputButtonEvent {
                        button: "left",
                        keymap: "R1",
                    })),
                button(icons::circle().size(48))
                    .width(65)
                    .height(65)
                    .on_press(Message::KodiReq(KodiCommand::InputButtonEvent {
                        button: "select",
                        keymap: "R1",
                    })),
                button(icons::chevron_right().size(48))
                    .width(65)
                    .height(65)
                    .on_press(Message::KodiReq(KodiCommand::InputButtonEvent {
                        button: "right",
                        keymap: "R1",
                    })),
            ]
            .spacing(5),
            row![
                button(icons::arrow_back().size(32))
                    .width(65)
                    .height(65)
                    .on_press(Message::KodiReq(KodiCommand::InputButtonEvent {
                        button: "back",
                        keymap: "R1",
                    })),
                button(icons::expand_more().size(48))
                    .width(65)
                    .height(65)
                    .on_press(Message::KodiReq(KodiCommand::InputButtonEvent {
                        button: "down",
                        keymap: "R1",
                    })),
                Space::new(65, 65),
            ]
            .spacing(5),
        ]
        .padding(10)
        .spacing(5)
        .align_x(iced::Alignment::Center),
    )
    .width(230)
    .into()
}
