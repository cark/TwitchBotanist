use crate::core::{ChatBot, ChatBotCommand::*};
use app_config::AppConfig;
use connect::TwitchChatConnector;
use std::error::Error;

extern crate websocket;

pub mod app_config;
mod connect;
mod core;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let app_config = AppConfig::new()?;

    let mut connector = TwitchChatConnector::new(&app_config).await;
    connector.send_message("Hello, world!")?;
    //self.connector.send_message("/followers")?; // not sure why we need this

    let mut chat_bot = ChatBot::new();

    loop {
        let messages = connector.recv_events()?;
        for message in messages {
            // NOTE: we'll need to consider timed bot events, but not right now
            if let Some(bot_command) = chat_bot.handle_event(message) {
                match bot_command {
                    SendMessage(message) => {
                        println!("Sending this message : {}", &message);
                        connector.send_message(&message)?;
                    }
                    LogTextMessage(message) => println!("{}", message),
                }
            }
        }
    }
}
