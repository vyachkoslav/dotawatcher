use std::env;
use std::sync::OnceLock;

use serenity::all::*;
use serenity::async_trait;
use tokio::time;
use tokio::time::Duration;
// use serenity::prelude::*;

macro_rules! get_string_for_status {
    ($status:expr) => {
        match $status {
            OnlineStatus::Offline => "не в сети",
            OnlineStatus::Idle => "спит",
            OnlineStatus::Invisible => "в невидимке",
            OnlineStatus::Online => "в сети",
            OnlineStatus::DoNotDisturb => "просит не беспокоить",
            _ => "непонятно",
        }
    };
}

static TARGET_GUILD: OnceLock<u64> = OnceLock::new();
static OUTPUT_CHANNEL: OnceLock<u64> = OnceLock::new();
static TARGET_USER: OnceLock<u64> = OnceLock::new();
static EMOJI_ID: OnceLock<u64> = OnceLock::new();
static ACTIVITY_STRING: OnceLock<String> = OnceLock::new();
static EMOJI_NAME: OnceLock<String> = OnceLock::new();

async fn main_loop(ctx: &Context) {
    let mut interval = time::interval(Duration::from_secs(5));
    let mut user: User = UserId::new(*TARGET_USER.get().unwrap())
        .to_user(ctx.http())
        .await
        .expect("Can't get target user");
    loop {
        match user.refresh(ctx.http()).await {
            Ok(()) => (),
            Err(err) => {
                eprintln!("Can't refresh target user {err}");
                interval.tick().await;
                continue;
            }
        };
        interval.tick().await;
    }
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.id == UserId::new(*TARGET_USER.get().unwrap()) {
            let reaction = ReactionType::Custom {
                animated: false,
                id: EmojiId::new(*EMOJI_ID.get().unwrap()),
                name: Some(EMOJI_NAME.get().unwrap().to_string()),
            };
            if let Err(why) = msg.react(&ctx.http, reaction).await {
                eprintln!("Error reacting to message: {why:?}");
            }
        }
    }

    async fn presence_update(&self, ctx: Context, new_data: Presence) {
        if new_data.guild_id != Some(GuildId::new(*TARGET_GUILD.get().unwrap())) {
            return;
        }

        // Username is not received when user is offline, so requesting it
        let user: Option<User> = match new_data.user.id.to_user(ctx.http()).await {
            Ok(u) => Some(u),
            Err(err) => {
                eprintln!("Couldn't receive user: {err:?}");
                None
            }
        };

        let username: &str = user.as_ref().map_or("непонятно кто", |u| &u.name);

        let mut message: CreateMessage = Default::default();
        let mut status: &str = get_string_for_status!(new_data.status);

        let device = new_data.client_status.map_or("", |device| {
            if let Some(s) = device.mobile {
                status = get_string_for_status!(s);
                "с телефона"
            } else if let Some(s) = device.web {
                status = get_string_for_status!(s);
                "с браузера"
            } else {
                status = get_string_for_status!(new_data.status);
                ""
            }
        });

        if new_data.activities.is_empty() {
            message = message.content(format!("{} теперь {} {}", username, status, device));
        } else {
            let activity = &new_data.activities[0];

            let activity_name: &str;
            let activity_details: &Option<String>;
            if activity.kind == ActivityType::Custom {
                activity_name = activity.details.as_deref().unwrap_or_default();
                activity_details = &None;
            } else {
                activity_name = &activity.name;
                activity_details = &activity.details;
            }

            let large_text: &Option<String>;
            let small_text: &Option<String>;
            if let Some(assets) = activity.assets.as_ref() {
                large_text = &assets.large_text;
                small_text = &assets.small_text;
            } else {
                large_text = &None;
                small_text = &None;
            }

            message = message.content(format!(
                "{} {} {} и шпилит в {}\n{}\n{}\n{}",
                username,
                status,
                device,
                activity_name,
                activity_details.as_deref().unwrap_or_default(),
                large_text.as_deref().unwrap_or_default(),
                small_text.as_deref().unwrap_or_default(),
            ));
        }
        if let Err(why) = ChannelId::new(*OUTPUT_CHANNEL.get().unwrap())
            .send_message(ctx.http(), message)
            .await
        {
            eprintln!("Error sending activity message: {why:?}");
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        let mut activity = ActivityData::custom("");
        activity.state = Some(ACTIVITY_STRING.get().unwrap().to_string());
        ctx.set_activity(Some(activity));
        // main_loop(&ctx).await
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    TARGET_GUILD
        .set(
            env::var("TARGET_GUILD")
                .expect("Expected TARGET_GUILD in the environment")
                .parse()
                .expect("TARGET_GUILD not a number"),
        )
        .unwrap();
    OUTPUT_CHANNEL
        .set(
            env::var("OUTPUT_CHANNEL")
                .expect("Expected OUTPUT_CHANNEL in the environment")
                .parse()
                .expect("OUTPUT_CHANNEL not a number"),
        )
        .unwrap();
    TARGET_USER
        .set(
            env::var("TARGET_USER")
                .expect("Expected TARGET_USER in the environment")
                .parse()
                .expect("TARGET_USER not a number"),
        )
        .unwrap();
    EMOJI_ID
        .set(
            env::var("EMOJI_ID")
                .expect("Expected EMOJI_ID in the environment")
                .parse()
                .expect("EMOJI_ID not a number"),
        )
        .unwrap();
    EMOJI_NAME
        .set(env::var("EMOJI_NAME").expect("Expected EMOJI_NAME in the environment"))
        .unwrap();
    ACTIVITY_STRING
        .set(env::var("ACTIVITY_STRING").expect("Expected ACTIVITY_STRING in the environment"))
        .unwrap();

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_PRESENCES;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        eprintln!("Client error: {why:?}");
    }
}
