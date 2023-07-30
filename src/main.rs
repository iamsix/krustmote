
use iced::executor;
use iced::theme::Theme;
use iced::font;
// use iced::time;
use iced::widget::{
    button, column, container, row, scrollable, Button, Space
};
//use iced::window;
use iced::{
    Color, Application, Command, 
    Element, Length, Settings,  //Subscription,
};

mod icons;
mod client;


fn main() -> iced::Result {
    println!("Hello, world!");
    Rustmote::run(Settings::default())
}

struct Rustmote {
    state: State,
    menu_width: u16,
    file_list: Vec<ListData>,

}

#[derive(Debug, Clone)]
pub struct ListData {
    label: String,
    on_click: Message, 
}

impl Application for Rustmote {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                state: State::Disconnected,
                menu_width: 150,
                file_list: Vec::new(),
            },
            font::load(include_bytes!("../fonts/MaterialIcons-Regular.ttf").as_slice())
                .map(Message::FontLoaded),
         //   Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Rustmote - ")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::ToggleLeftMenu => {
                // TODO : Fancy animation by subtracting until 0 etc. maybe.
                self.menu_width = if self.menu_width == 0 {150} else {0};
            }

            Message::ServerStatus(event) => match event {
                client::Event::Connected(connection) => {
                    self.state = State::Connected(connection);
                }
                client::Event::Disconnected => {
                    self.state = State::Disconnected;
                }
                client::Event::UpdateFileList { data } => {
                    self.file_list = data;
                }

            }
            Message::KodiReq(command) => {
                match &mut self.state {
                    State::Connected(connection) => {
                        connection.send(command);

                    }
                    State::Disconnected => {
                        println!("Kodi is apparently disconnected so I can't");
                    }
                }

            }

            _ => {}
        }
        Command::none()
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        client::connect().map(Message::ServerStatus)
    }

    fn view(&self) -> Element<Message> {

        let content = column! [
            // Top Bar thing
            top_bar(self),
            row![
                // Left (menu)
                left_menu(self).explain(Color::from_rgb8(0, 255, 0)),
                //Center (content)
                center_area(self),
                // Right (remote)
                remote(self),
            ],
            // TODO: Now playing bar goes here.

        ];
        
        let x: Element<_> = container(content).into();

        x //.explain(Color::from_rgb8(255, 0, 0))
        
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
    }



}

#[derive(Debug, Clone)]
enum Message{
    FontLoaded(Result<(), font::Error>),
    ToggleLeftMenu,
    ServerStatus(client::Event),
    KodiReq(client::KodiCommand), // - KodiCommand being an enum likely
}


enum State {
    Disconnected,
    Connected(client::Connection),
}

// TODO : Move these somewhere else / to a different file/struct/etc
fn top_bar<'a>(rustmote: &Rustmote) -> Element<'a, Message> {
    container(
        row![
            button("=").on_press(Message::ToggleLeftMenu),
            Space::new(Length::Fill, Length::Shrink),
            match rustmote.state {
                State::Disconnected => icons::sync_disabled(),
                _ => icons::sync()
            },
        ]
    )
    .into()
}

fn center_area<'a>(rustmote: &'a Rustmote) -> Element<'a, Message> {
    scrollable(
        column(
            rustmote.file_list
            .iter()
         //   .map(|x| x.title.as_str())
            .map(|x| Button::new(
                x.label.as_str()
                ).on_press(x.on_click.clone())
            )
            .map(Element::from)
            .collect()
        )
        .spacing(20)
        .padding(20),
    )
    .width(Length::Fill)
    .into()
}

fn left_menu<'a>(rustmote: &Rustmote) -> Element<'a, Message> {
    container(
        column![
            button(row![icons::folder(), "Files"])
                .on_press(Message::KodiReq(
                    client::KodiCommand::GetSources(
                        client::MediaType::Video)
                    )
                ),
            button("Settings")
        ]
        
        .spacing(0)
        .padding(10),
    )
    .max_width(rustmote.menu_width)
    .into()
}

fn remote<'a>(_rustmote: &Rustmote) -> Element<'a, Message> {
    container(
        column![
            button(icons::bug_report())
                .on_press(Message::KodiReq(client::KodiCommand::Test)),
            button(icons::expand_less().size(48)).width(65).height(65), // center this
            row![
                button(icons::chevron_left().size(48)).width(65).height(65),
                button(icons::circle().size(48)).width(65).height(65),
                button(icons::chevron_right().size(48)).width(65).height(65),
            ].spacing(5),
            row![
                button(icons::arrow_back().size(32)).width(65).height(65),
                button(icons::expand_more().size(48)).width(65).height(65),
            ].spacing(5),
        ]
        .padding(10)
        .spacing(5),
    )
    .width(220)
    .into()
}
// END TODO
