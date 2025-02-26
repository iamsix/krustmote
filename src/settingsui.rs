use iced::Color;
use iced::Element;
use iced::Length;
use iced::Task as Command;
use iced::widget::{Space, button, column, row, text, text_input};
use std::net::IpAddr;
use std::sync::Arc;

use crate::koditypes::KodiServer;

#[derive(Clone)]
pub struct Settings {
    // Serverlist/ etc to select servers eventually
    edit_server_name: String,
    edit_server_ip: String,
    edit_server_ws_port: String,
    edit_server_http_port: String,
    edit_server_username: String,
    edit_server_password: String,
    name_is_valid: bool,
    ip_is_valid: bool,
    ws_port_is_valid: bool,
    http_port_is_valid: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    ServerNameChanged(String),
    ServerIPChanged(String),
    ServerWsPortChanged(String),
    ServerHttpPortChanged(String),
    ServerUnChanged(String),
    ServerPwChanged(String),
    SubmitForm,
    Cancel,
}

#[derive(Debug, Clone)]
pub enum Event {
    AddServer(KodiServer),
    Cancel,
}

impl Settings {
    pub fn new() -> Self {
        Settings {
            edit_server_name: "kodi".to_string(),
            edit_server_ip: "127.0.0.1".to_string(),
            edit_server_ws_port: "9090".to_string(),
            edit_server_http_port: "8080".to_string(),
            edit_server_username: "".to_string(),
            edit_server_password: "".to_string(),
            name_is_valid: true,
            ip_is_valid: true,
            ws_port_is_valid: true,
            http_port_is_valid: true,
        }
    }

    pub fn load(server: Arc<KodiServer>) -> Self {
        Settings {
            edit_server_name: server.name.clone(),
            edit_server_ip: server.ip.clone(),
            edit_server_ws_port: server.websocket_port.to_string(),
            edit_server_http_port: server.webserver_port.to_string(),
            edit_server_username: server.username.clone(),
            edit_server_password: server.password.clone(),
            name_is_valid: true,
            ip_is_valid: true,
            ws_port_is_valid: true,
            http_port_is_valid: true,
        }
    }

    pub fn update(&mut self, message: Message) -> Command<Event> {
        match message {
            Message::ServerNameChanged(name) => {
                self.name_is_valid = !name.is_empty();
                self.edit_server_name = name
            }
            Message::ServerIPChanged(ip) => {
                let addr = ip.parse::<IpAddr>();
                self.ip_is_valid = addr.is_ok();
                self.edit_server_ip = ip
            }
            Message::ServerWsPortChanged(port) => {
                let valid = port.parse::<u16>();
                self.ws_port_is_valid = valid.is_ok();
                self.edit_server_ws_port = port
            }
            Message::ServerHttpPortChanged(port) => {
                let valid = port.parse::<u16>();
                self.http_port_is_valid = valid.is_ok();
                self.edit_server_http_port = port
            }
            Message::ServerUnChanged(un) => self.edit_server_username = un,
            Message::ServerPwChanged(pw) => self.edit_server_password = pw,
            Message::SubmitForm => {
                let ws_port: u16 = self
                    .edit_server_ws_port
                    .parse()
                    .expect("String should already be validated");
                let http_port: u16 = self
                    .edit_server_http_port
                    .parse()
                    .expect("String should already be validated");
                let server = KodiServer::new(
                    self.edit_server_name.clone(),
                    self.edit_server_ip.clone(),
                    ws_port,
                    http_port,
                    self.edit_server_username.clone(),
                    self.edit_server_password.clone(),
                );
                return Command::perform(async {}, move |_| Event::AddServer(server.clone()));
            }
            Message::Cancel => {
                return Command::perform(async {}, |_| Event::Cancel);
            }
        }
        Command::none()
    }

    pub fn view<'a>(&'a self) -> Element<'a, Message> {
        let red = Color::from_rgb8(255, 0, 0);
        column![
            if self.name_is_valid {
                text("Server Name:")
            } else {
                text("Server Name:").color(red)
            },
            text_input("Livingroom", &self.edit_server_name).on_input(Message::ServerNameChanged),
            if self.ip_is_valid {
                text("Server IP:")
            } else {
                text("Server IP:").color(red)
            },
            text_input("127.0.0.1", &self.edit_server_ip).on_input(Message::ServerIPChanged),
            if self.ws_port_is_valid {
                text("Server Websocket port:")
            } else {
                text("Server Websocket port:").color(red)
            },
            text_input("9090", &self.edit_server_ws_port).on_input(Message::ServerWsPortChanged),
            if self.http_port_is_valid {
                text("Server Web/HTTP port:")
            } else {
                text("Server Web/HTTP port:").color(red)
            },
            text_input("8080", &self.edit_server_http_port)
                .on_input(Message::ServerHttpPortChanged),
            text("Username:"),
            text_input("", &self.edit_server_username).on_input(Message::ServerUnChanged),
            text("Password"),
            text_input("", &self.edit_server_password).on_input(Message::ServerPwChanged),
            row![
                Space::new(Length::Fill, 10),
                button("Cancel").on_press(Message::Cancel),
                if self.ip_is_valid
                    && self.ws_port_is_valid
                    && self.http_port_is_valid
                    && self.name_is_valid
                {
                    button("Save").on_press(Message::SubmitForm)
                } else {
                    button("Save")
                }
            ]
            .padding(10)
            .spacing(10)
        ]
        .into()
    }
}
