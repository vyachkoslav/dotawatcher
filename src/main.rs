use std::env;
use std::time::Duration;

use lazy_static::lazy_static;

use serenity::all::*;
use serenity::async_trait;
use tokio::time;
// use serenity::prelude::*;

lazy_static!{
    static ref TARGET_GUILD: u64 = env::var("TARGET_SERVER").expect("Expected TARGET_SERVER in the environment").parse().expect("TARGET_SERVER not a number");
    static ref OUTPUT_CHANNEL: u64 = env::var("OUTPUT_CHANNEL").expect("Expected OUTPUT_CHANNEL in the environment").parse().expect("OUTPUT_CHANNEL not a number");
    static ref TARGET_USER: u64 = env::var("TARGET_USER").expect("Expected TARGET_USER in the environment").parse().expect("TARGET_USER not a number");
    static ref EMOJI_ID: u64 = env::var("EMOJI_ID").expect("Expected EMOJI_ID in the environment").parse().expect("EMOJI_ID not a number");
    static ref EMOJI_NAME: String = env::var("EMOJI_NAME").expect("Expected EMOJI_NAME in the environment");
    static ref ACTIVITY_STRING: String = env::var("ACTIVITY_STRING").expect("Expected ACTIVITY_STRING in the environment");
}

async fn main_loop(ctx: &Context) {
    let mut interval = time::interval(Duration::from_secs(5));
    let mut user: User = UserId::new(*TARGET_USER).to_user(ctx.http()).await.expect("Can't get target user");
    loop {
        match user.refresh(ctx.http()).await {
            Ok(()) => (),
            Err(err) => {
                eprintln!("Can't refresh target user {err}");
                interval.tick().await;
                continue;
            },
        };
        interval.tick().await; 
    }
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
                eprintln!("Error reacting to message: {why:?}");
            }
        }
    }

    async fn presence_update(&self, ctx: Context, new_data: Presence) {
        if new_data.guild_id == Some(GuildId::new(*TARGET_GUILD)) {
            return;
        }

        // Username is not received when user is offline, so requesting it
        let user: Option<User> = new_data.user.id.to_user(ctx.http()).await.ok();
        let username = if let Some(user) = user { user.name } else { "непонятно кто".to_string() };

        let mut message: CreateMessage = Default::default();
        if new_data.activities.is_empty() {
            let status = if new_data.status == OnlineStatus::Offline {
                    "не в сети"
                } else {
                    "в сети"
                };
            message = message.content(format!("{} теперь {}", username, status));
        } else {
            message = message.content(format!("{} шпилит в {}", username, new_data.activities[0].name));
        }
        if let Err(why) = ChannelId::new(*OUTPUT_CHANNEL).send_message(ctx.http(), message).await {
            eprintln!("Error sending activity message: {why:?}");
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        ctx.set_activity(Some(ActivityData::playing((*ACTIVITY_STRING).clone())));
        // main_loop(&ctx).await
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_PRESENCES;

    let mut client =
        Client::builder(&token, intents).event_handler(Handler).await.expect("Err creating client");

    if let Err(why) = client.start().await {
        eprintln!("Client error: {why:?}");
    }
}
