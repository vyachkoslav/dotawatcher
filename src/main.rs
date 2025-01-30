use std::collections::HashMap;
use std::env;
use std::sync::{Arc, OnceLock};

use serenity::all::{
    ActivityData, ActivityType, CacheHttp, ChannelId, Client, Context, CreateMessage, EmojiId,
    EventHandler, GatewayIntents, GuildId, Http, Message, OnlineStatus, Presence, ReactionType,
    Ready, UserId,
};
use serenity::async_trait;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{self, Duration};

use anyhow::{anyhow, Result};
use serde::Deserialize;

macro_rules! get_string_for_status {
    ($status:expr) => {
        match $status {
            OnlineStatus::Offline => &LOCALIZATION.get().unwrap().offline,
            OnlineStatus::Idle => &LOCALIZATION.get().unwrap().idle,
            OnlineStatus::Invisible => &LOCALIZATION.get().unwrap().invisible,
            OnlineStatus::Online => &LOCALIZATION.get().unwrap().online,
            OnlineStatus::DoNotDisturb => &LOCALIZATION.get().unwrap().donotdisturb,
            _ => &LOCALIZATION.get().unwrap().unknown,
        }
    };
}

macro_rules! set_env_num {
    ($var:expr) => {
        let var_str = stringify!($var);
        $var.set(
            env::var(var_str)
                .expect("Expected {var_str} in the environment")
                .parse()
                .expect("{var_str} not a number"),
        )
        .unwrap();
    };
}

macro_rules! set_env_str {
    ($var:expr) => {
        let var_str = stringify!($var);
        $var.set(env::var(var_str).expect("Expected {var_str} in the environment"))
            .unwrap();
    };
}

static STEAM_REQUEST_URL: OnceLock<String> = OnceLock::new();

static TARGET_GUILD: OnceLock<u64> = OnceLock::new();
static OUTPUT_CHANNEL: OnceLock<u64> = OnceLock::new();
static TARGET_USER: OnceLock<u64> = OnceLock::new();
static TARGET_STEAMID32: OnceLock<u64> = OnceLock::new();
static EMOJI_ID: OnceLock<u64> = OnceLock::new();
static EMOJI_NAME: OnceLock<String> = OnceLock::new();
static LOCALIZATION: OnceLock<Localization> = OnceLock::new();

static HEROES: OnceLock<HashMap<i64, String>> = OnceLock::new();

const DOTA_LOOP_INTERVAL: Duration = Duration::from_secs(60);
const STEAM_LOOP_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct Localization {
    pub bot_activity: String,
    pub plays: String,

    pub won: String,
    pub lost: String,
    pub played_on: String,
    pub with_score: String,
    pub match_duration: String,
    pub minutes: String,

    pub target_name: String,
    pub offline: String,
    pub idle: String,
    pub invisible: String,
    pub online: String,
    pub donotdisturb: String,
    pub unknown: String,

    pub using_phone: String,
    pub using_browser: String,
    pub using_computer: String,

    pub on_steam: String,
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct Response<T> {
    pub items: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct Hero {
    pub id: i64,
    pub localized_name: String,
}

#[derive(Debug, Deserialize)]
struct MatchData {
    pub match_id: i64,
    pub player_slot: i64,
    pub radiant_win: bool,
    pub hero_id: i64,
    pub duration: i64,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
}

#[derive(Debug, Deserialize)]
struct SteamUserData {
    pub personastate: i64,
    pub gameextrainfo: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponsePlayers {
    pub players: Vec<SteamUserData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename(deserialize = "response"))]
struct SteamResponse {
    pub response: ResponsePlayers,
}

#[derive(Debug, Deserialize)]
struct PlayerState {
    pub status: OnlineStatus,
    pub game: Option<String>,
}

async fn get_steam_state() -> Result<PlayerState> {
    let body = reqwest::get(STEAM_REQUEST_URL.get().unwrap())
        .await?
        .text()
        .await?;
    let mut response: SteamResponse = serde_json::from_str(&body)?;
    if response.response.players.len() == 0 {
        return Err(anyhow!("Can't find player with this ID"));
    }
    let data = response.response.players.remove(0);
    let state = PlayerState {
        status: match data.personastate {
            0 => OnlineStatus::Offline,
            1 => OnlineStatus::Online,
            2 => OnlineStatus::DoNotDisturb,
            3 => OnlineStatus::Idle,
            _ => OnlineStatus::Offline,
        },
        game: data.gameextrainfo,
    };

    Ok(state)
}

async fn set_heroes() -> Result<()> {
    let body = reqwest::get("https://api.opendota.com/api/heroes")
        .await?
        .text()
        .await?;
    let mut heroes_hm: HashMap<i64, String> = HashMap::new();
    let heroes: Response<Hero> = serde_json::from_str(&body)?;
    for hero in heroes.items {
        heroes_hm.insert(hero.id, hero.localized_name);
    }
    if HEROES.set(heroes_hm).is_err() {
        return Err(anyhow!("Couldn't set HEROES"));
    }
    Ok(())
}

async fn request_matches(url: &str) -> Result<Vec<MatchData>> {
    let body = reqwest::get(url).await?.text().await?;
    let response: Response<MatchData> = serde_json::from_str(&body)?;
    Ok(response.items)
}

async fn steamwatcher_loop(http: &Http, current_state: &Mutex<PlayerState>) {
    println!("Steam watcher enabled");
    let mut interval = time::interval(STEAM_LOOP_INTERVAL);

    loop {
        interval.tick().await;
        let state = get_steam_state().await;
        if let Ok(state) = state {
            let mut cur_state = current_state.lock().await;
            let game_state_eq = (*cur_state).game == state.game || state.game.is_none();
            let no_new_status = (*cur_state).status != OnlineStatus::Offline
                || state.status == OnlineStatus::Offline;
            if game_state_eq && no_new_status {
                continue;
            }
            let status: &str = get_string_for_status!(state.status);
            let content: String;
            if game_state_eq {
                content = format!(
                    "{} {} {}",
                    &LOCALIZATION.get().unwrap().target_name,
                    status,
                    &LOCALIZATION.get().unwrap().on_steam,
                );
            } else {
                content = format!(
                    "{} {} {} {} {}",
                    &LOCALIZATION.get().unwrap().target_name,
                    status,
                    &LOCALIZATION.get().unwrap().plays,
                    state.game.as_deref().unwrap_or_default(),
                    &LOCALIZATION.get().unwrap().on_steam,
                );
            }
            (*cur_state).game = state.game;
            if !no_new_status {
                (*cur_state).status = state.status;
            }
            drop(cur_state);

            let mut message: CreateMessage = Default::default();
            message = message.content(content);
            message = message.tts(true);

            if let Err(why) = ChannelId::new(*OUTPUT_CHANNEL.get().unwrap())
                .send_message(http, message)
                .await
            {
                eprintln!("Error sending Steam activity message: {why:?}");
            }
        }
    }
}
async fn dotawatcher_loop(http: &Http) {
    println!("Dota watcher enabled");
    let mut interval = time::interval(DOTA_LOOP_INTERVAL);
    let mut last_match_id = 0;
    let locals = &LOCALIZATION.get().unwrap();
    let matches_url = format!(
        "https://api.opendota.com/api/players/{}/recentMatches",
        &TARGET_STEAMID32.get().unwrap()
    );
    loop {
        interval.tick().await;

        if HEROES.get().is_none() {
            if let Err(err) = set_heroes().await {
                eprintln!("Error fetching heroes: {err}");
                continue;
            }
        }

        let matches = match request_matches(&matches_url).await {
            Ok(matches) => matches,
            Err(err) => {
                eprintln!("Couldn't fetch matches: {err}");
                continue;
            }
        };
        let last = match matches.first() {
            Some(last) => last,
            None => {
                eprintln!("Empty matches list");
                continue;
            }
        };
        if last.match_id == last_match_id {
            continue;
        }

        if last_match_id == 0 {
            last_match_id = last.match_id;
            continue;
        }
        last_match_id = last.match_id;

        let result = if last.radiant_win == (last.player_slot < 5) {
            &locals.won
        } else {
            &locals.lost
        };

        let content = format!(
            "{target_name} {result}. {played_on} {hero} {with_score} {kills}, {deaths}, {assists}. {match_duration} {minutes} {minutes_str}.",
            target_name = locals.target_name,
            result = result,
            hero = HEROES.get().unwrap().get(&last.hero_id).unwrap(),
            kills = last.kills,
            deaths = last.deaths,
            assists = last.assists,
            minutes = last.duration / 60,
            played_on = locals.played_on,
            with_score = locals.with_score,
            match_duration = locals.match_duration,
            minutes_str = locals.minutes,
        );
        let mut message: CreateMessage = Default::default();
        message = message.content(content);
        message = message.tts(true);

        if let Err(why) = ChannelId::new(*OUTPUT_CHANNEL.get().unwrap())
            .send_message(http, message)
            .await
        {
            eprintln!("Error sending dota message: {why:?}");
        }
    }
}

struct Handler {
    dotawatcher_thread: Mutex<Option<JoinHandle<()>>>,
    steamwatcher_thread: Mutex<Option<JoinHandle<()>>>,
    last_message: Mutex<Option<String>>,
    current_state: Arc<Mutex<PlayerState>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.id == UserId::new(*TARGET_USER.get().unwrap()) {
            let reaction = ReactionType::Custom {
                animated: false,
                id: EmojiId::new(*EMOJI_ID.get().unwrap()),
                name: Some(EMOJI_NAME.get().unwrap().clone()),
            };
            if let Err(why) = msg.react(&ctx.http, reaction).await {
                eprintln!("Error reacting to message: {why:?}");
            }
        }
    }

    async fn presence_update(&self, ctx: Context, mut new_data: Presence) {
        if new_data.guild_id != Some(GuildId::new(*TARGET_GUILD.get().unwrap()))
            || new_data.user.id != *TARGET_USER.get().unwrap()
        {
            return;
        }

        let username = &LOCALIZATION.get().unwrap().target_name;

        let mut message: CreateMessage = Default::default();
        message = message.tts(true);

        let content: String;

        let mut status: &str = get_string_for_status!(new_data.status);

        let device = new_data.client_status.map_or("", |device| {
            if let Some(s) = device.mobile {
                status = get_string_for_status!(s);
                &LOCALIZATION.get().unwrap().using_phone
            } else if let Some(s) = device.web {
                status = get_string_for_status!(s);
                &LOCALIZATION.get().unwrap().using_browser
            } else {
                status = get_string_for_status!(new_data.status);
                &LOCALIZATION.get().unwrap().using_computer
            }
        });

        let mut state = self.current_state.lock().await;
        (*state).status = new_data.status;
        if new_data.activities.is_empty() {
            content = format!("{} {}{}", username, status, device);
            (*state).game = None;
        } else {
            let activity = new_data.activities.remove(0);

            let activity_name: Option<String>;
            let activity_details: Option<String>;
            if activity.kind == ActivityType::Custom {
                activity_name = activity.details;
                activity_details = None;
            } else {
                activity_name = Some(activity.name);
                activity_details = activity.details;
            }

            if activity_name == (*state).game {
                return;
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

            content = format!(
                "{} {}{} {} {}\n{}\n{}\n{}",
                username,
                status,
                device,
                &LOCALIZATION.get().unwrap().plays,
                activity_name.as_deref().unwrap_or_default(),
                activity_details.as_deref().unwrap_or_default(),
                large_text.as_deref().unwrap_or_default(),
                small_text.as_deref().unwrap_or_default(),
            );
            (*state).game = activity_name;
        }
        drop(state);

        let mut last = self.last_message.lock().await;
        if Some(&content) == (*last).as_ref() {
            return;
        }

        message = message.content(&content);

        if let Err(why) = ChannelId::new(*OUTPUT_CHANNEL.get().unwrap())
            .send_message(ctx.http(), message)
            .await
        {
            eprintln!("Error sending activity message: {why:?}");
        }
        *last = Some(content);
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        let mut activity = ActivityData::custom("");
        activity.state = Some(LOCALIZATION.get().unwrap().bot_activity.clone());
        ctx.set_activity(Some(activity));

        let mut dotawatcher_thread = self.dotawatcher_thread.lock().await;
        if let Some(thread) = &*dotawatcher_thread {
            thread.abort();
        }
        let mut steamwatcher_thread = self.steamwatcher_thread.lock().await;
        if let Some(thread) = &*steamwatcher_thread {
            thread.abort();
        }

        let http = ctx.http.clone();
        *dotawatcher_thread = Some(tokio::spawn(async move {
            dotawatcher_loop(&http).await;
        }));

        let http = ctx.http.clone();
        let current_state = self.current_state.clone();
        *steamwatcher_thread = Some(tokio::spawn(async move {
            steamwatcher_loop(&http, &current_state).await;
        }));
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Expected DISCORD_TOKEN in the environment");

    set_env_num!(TARGET_GUILD);
    set_env_num!(OUTPUT_CHANNEL);
    set_env_num!(TARGET_USER);
    set_env_num!(TARGET_STEAMID32);
    set_env_num!(EMOJI_ID);
    set_env_str!(EMOJI_NAME);

    let _ = STEAM_REQUEST_URL.set(format!(
        "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v0002/?key={}&steamids={}",
        env::var("STEAM_TOKEN").expect("Expected STEAM_TOKEN in the environment"),
        env::var("TARGET_STEAMID64").expect("Expected TARGET_STEAMID64 in the environment")
    ));
    let locals: Localization = serde_json::from_str(
        &std::fs::read_to_string("localization.json")
            .expect("localization.json file in the root folder"),
    )
    .unwrap_or_else(|err| panic!("Invalid localization.json: {err}"));
    LOCALIZATION.set(locals).unwrap();

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_PRESENCES;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler {
            dotawatcher_thread: Mutex::new(None),
            steamwatcher_thread: Mutex::new(None),
            last_message: Mutex::new(None),
            current_state: Arc::new(Mutex::new(PlayerState {
                game: None,
                status: OnlineStatus::Offline,
            })),
        })
        .await
        .expect("Successfull client creation");

    if let Err(why) = client.start().await {
        eprintln!("Client error: {why:?}");
    }
}
