use std::env;

use lazy_static::lazy_static;

use serenity::all::*;
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
// use serenity::prelude::*;

lazy_static!{
    static ref OUTPUT_CHANNEL: u64 = env::var("OUTPUT_CHANNEL").expect("Expected OUTPUT_CHANNEL in the environment").parse().expect("OUTPUT_CHANNEL not a number");
    static ref TARGET_USER: u64 = env::var("TARGET_USER").expect("Expected TARGET_USER in the environment").parse().expect("TARGET_USER not a number");
    static ref EMOJI_ID: u64 = env::var("EMOJI_ID").expect("Expected EMOJI_ID in the environment").parse().expect("EMOJI_ID not a number");
    static ref EMOJI_NAME: String = env::var("EMOJI_NAME").expect("Expected EMOJI_NAME in the environment");
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.id == UserId::new(*TARGET_USER) {
            let reaction = ReactionType::Custom {
                animated: false,
                id: EmojiId::new(*EMOJI_ID),
                name: Some((*EMOJI_NAME).clone()) 
            };
            if let Err(why) = msg.react(&ctx.http, reaction).await {
                println!("Error sending message: {why:?}");
            }
        }
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client =
        Client::builder(&token, intents).event_handler(Handler).await.expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}
