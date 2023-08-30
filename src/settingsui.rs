use iced::widget::{button, column, text, text_input};
use iced::Element;
use iced::{Color, Command};
use std::net::IpAddr;

use crate::koditypes::KodiServer;

#[derive(Clone)]
pub struct Settings {
    // DBSend? Not sure if I want to bypass that entirely
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
}

#[derive(Debug, Clone)]
pub enum Event {
    AddServer(KodiServer)
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
                return Command::perform(async {}, |_| Event::AddServer(server));
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
                text("Server Name:").style(red)
            },
            text_input("Livingroom", &self.edit_server_name).on_input(Message::ServerNameChanged),
            if self.ip_is_valid {
                text("Server IP:")
            } else {
                text("Server IP:").style(red)
            },
            text_input("127.0.0.1", &self.edit_server_ip).on_input(Message::ServerIPChanged),
            if self.ws_port_is_valid {
                text("Server Websocket port:")
            } else {
                text("Server Websocket port:").style(red)
            },
            text_input("9090", &self.edit_server_ws_port).on_input(Message::ServerWsPortChanged),
            if self.http_port_is_valid {
                text("Server Web/HTTP port:")
            } else {
                text("Server Web/HTTP port:").style(red)
            },
            text_input("8080", &self.edit_server_http_port)
                .on_input(Message::ServerHttpPortChanged),
            text("Username:"),
            text_input("", &self.edit_server_username).on_input(Message::ServerUnChanged),
            text("Password"),
            text_input("", &self.edit_server_password).on_input(Message::ServerPwChanged),
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
        .into()
    }
}
