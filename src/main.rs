/**

    Definitions:
        - Source Channel: A channel which collects messages in an expected
            language. The program expects to be configured with multiple Source
            Channels, which are mapped to their expected language codes. The
            language codes we use are those supported by deepl's API.
            https://www.deepl.com/docs-api/translating-text/request/

        - Source Message: A message in the Source Channel.

        - Source Bot Message: A bot message posted into the Source Channel.

        - Source Bot Reply: A reply to a Source Message by the Bot, always is
            the result of an Aggregate Reply.

        - Aggregate Channel: A channel into which translations of messages in
            the Source Channels are posted.

        - Aggregate Bot Message: A message posted into the Aggregate Channel by
            the bot.

        - Aggregate Reply: A reply in the Aggregate channel to an Aggregate Bot
            Message.


    V2:
        - Messages which are translated by babelfish should be posted into a
            configurable channel by the bot (this channel would likely be viewable
            by admins of the server).
        - A reply to the forwarded message should be translated back into the source
            langauge, and posted back to the original channel by the bot, ensuring
            that we denote the replier's name (such that they can be more easily
            tagged?).


*/



// Data Storage
use std::{collections::HashMap, sync::Arc};

// Useful http things
use serde::Deserialize;
use serde_json::{from_str};
use reqwest;

// Discord Client
use serenity::{async_trait, client::{Context, EventHandler, ClientBuilder}, model::{channel::{Message, MessageReference}, gateway::Ready, id::{MessageId, ChannelId, UserId}}, prelude::{RwLock,TypeMapKey}};

// DeepL returns a Vec<Translation>, so we deserialise through two types, a
// container (DeepLResponse) and an individual item (Translation)
#[derive(Deserialize, Debug, Clone)]
pub struct Translation {
    text: String,
    detected_source_language: String
}
#[derive(Deserialize, Debug, Clone)]
struct DeepLResponse {
    translations: Vec<Translation>
}
#[derive(Deserialize, Debug, Clone)]
struct PastTranslation {
    channel_id: ChannelId,
    message_id: MessageId,
    language: String
}
struct BotMessage {
    target_channel_id: ChannelId,
    target_language: String,
    target_reply_to_message: MessageId
}

// A map of MessageId => String
struct Translations;

// The thing we're storing is a rw-locked HashMap. wrapped in an Arc for thread
// safety
impl TypeMapKey for Translations {
    type Value = Arc<RwLock<HashMap<MessageId, PastTranslation>>>;
}

// I would like this to be a config struct I guess?
#[derive(Deserialize, Debug, Default, Clone)]
struct AppConfig {
    bot_token: String,
    bot_user_id: UserId,
    deepl_api_key: String,
    aggregate_channel_id: ChannelId,
    source_channel_language: HashMap<ChannelId, String>,
    default_language: String,
}

impl TypeMapKey for AppConfig {
    type Value = Arc<AppConfig>;
}

#[derive(Default)]
struct ReplyTo {
    message: Option<Message>,
    message_id: MessageId,
    author_id: UserId
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {

        // We don't need the RwLock on this value since we're not writing to the
        // variable. It does not need to be "super" thread safe
        let config = {
            let data = ctx.data.read().await;
            data.get::<AppConfig>().expect("something").clone()
        };

        println!("Got message from {:?} ({}): {}", msg.author.id, msg.author.name, msg.content.clone());
        // Don't care about messages from self.
        if is_bot_message(config.bot_user_id, msg.author.id) {
            println!("This is the bot, ignoring.");
            return
        }
        if !is_monitored_channel(config.aggregate_channel_id, config.source_channel_language.clone(), msg.channel_id.clone()) {
            println!("This message is in a non-configured channel, ignoring");
            return
        }

        // Get a reference to the replied-to message (if any).
        // how to get an empty message here?
        let reply_to = get_replied_to_message(msg.clone().referenced_message);

        println!("This is a reply to message {}", reply_to.message_id);

        // If we are posting in the aggregate channel and we are not replying
        // to anyone, we don't want to make a babelfish message

        if msg.channel_id == config.aggregate_channel_id && reply_to.message_id == 0 {
            println!("This message in the aggregate channel is not a reply, ignoring.");
            return;
        };

        // If we have the channel id in the config map, we should get the
        // source language here
        let channel_lang;
        if config.source_channel_language.contains_key(&msg.channel_id) {
            channel_lang = String::from(config.source_channel_language.get(&msg.channel_id).unwrap());
            println!("Found message in channel {:?} with expected source language {}", msg.channel_id, channel_lang);
        } else if msg.channel_id != config.aggregate_channel_id {
            println!("The bot is not active in the channel with ID: {}", msg.channel_id);
            return;
        } else {
            channel_lang = config.default_language.clone();
        }

        // Get a the thread-safe lock on the translations from the context's data store
        let translations_lock = {
            // We need to read the data first, so let's do that for now.
            // Careless use of write locks could cause our program to lock.
            let data_read = ctx.data.read().await;

            // Cloning the value will not duplicate the data, just the reference
            // Wrapping the value in Arc means we can keep the data lock open
            // for minimal time
            data_read.get::<Translations>().expect("Expected something").clone()
        };

        let default_language = &config.default_language;

        // Unless we discern otherwise, a message in this channel should be
        // translated into the operator_language and result in an Aggregate Bot
        // Message.
        let mut target_message = BotMessage {
            target_channel_id: ChannelId::from(config.aggregate_channel_id),
            target_language: String::from(default_language),
            // A reply to the BotMessage should result in the _next_ BotMessage
            // replying to the original message!
            target_reply_to_message: MessageId::from(msg.id)
        };

        // We may however want to send a Source Bot Reply, so we should check
        // that a little bit later

        // If the message we are replying to is an Aggregate Bot Message, then
        // we are likely to want to send a Source Bot Reply as a result of the
        // translation.

        // Which means our data structure must contain both the Aggregate Bot
        // and Source Bot message IDs.

        // Source Message -> Aggregate Bot Message
        // Source Reply -> Aggregate Bot Reply
        // Aggregate Message -> Source Bot Message
        // Aggregate Reply -> Source Bot Reply

        // Now we need to find out if the replied-to message has been translated
        // already. If it has, we'll translate back to its source language.
        // To do this, we need to activate our read lock on the data, then use
        // it to overwrite the default target language which was derived from
        // the channel name
        {
            let all_past_translations = translations_lock
                .read()
                .await;

            let referenced_past_translation = all_past_translations
                .get(&reply_to.message_id);

            target_message = match referenced_past_translation {
                Some(s) => BotMessage {
                    // The target channel ID is the inverse.
                    // Source -> Aggregate
                    // Aggregate -> Source
                    target_channel_id: s.channel_id,
                    target_language: s.language.clone(),
                    target_reply_to_message: s.message_id
                },
                None => target_message
            };
        };

        // If the message we are replying to was _not_ written by the bot, we
        // should be printing this in the aggregate channel, not the source
        match reply_to.message.clone() {
            Some(replying_to) => {
                if
                    replying_to.id != 0 &&
                    replying_to.author.id != config.bot_user_id &&
                    msg.channel_id != config.aggregate_channel_id
                {
                    // ideally here we would be replying to the original translation
                    // in the aggregate_channel, but that's just going to melt my brain
                    target_message.target_reply_to_message = MessageId::from(0);
                    target_message.target_channel_id = config.aggregate_channel_id;
                    target_message.target_language = config.default_language.clone()
                }
            },
            None => {}
        }

        println!("Translating to {}, then sending a message to channel {}", &target_message.target_language, target_message.target_channel_id);

        // Go do the translation with deepL
        let translation = translate_message(
            msg.content.clone(),
            String::from(&target_message.target_language),
            &config.deepl_api_key
        ).await;

        let past_translation = PastTranslation {
            language: channel_lang.clone(),
            channel_id: msg.channel_id,
            message_id: msg.id
        };

        // Now write this message's id to storage, keying its source language
        {
            let mut translations = translations_lock.write().await;
            translations.entry(msg.id.clone()).or_insert(past_translation.clone());
            println!("Stored the message {:?} with key {}", past_translation.clone(), msg.id.clone());
        };

        let from_name = match msg.clone().author_nick(&ctx.http).await {
            Some(_nickname) => _nickname,
            None => msg.clone().author.name
        };

        let sent_message_result = target_message.target_channel_id.send_message(&ctx.http, |f| {

            let content = format!("{} (from: {})", translation.text, from_name);
            let mut message_builder = f.content(content);

            // If `msg` is a reply TO A BOT MESSAGE, we want to attach the built
            // message to something
            if
                reply_to.message_id != 0 &&
                is_bot_message(config.bot_user_id, reply_to.author_id)
            {
                println!("This message is a reply to {} in the channel {}", target_message.target_reply_to_message, target_message.target_channel_id);
                let msg_ref = MessageReference::from((target_message.target_channel_id, target_message.target_reply_to_message));
                message_builder = message_builder.reference_message(msg_ref);
            }

            message_builder
        }).await;

        if let Err(why) = sent_message_result {
            println!("Error sending message: {:?}", why);
        } else {
            let sent_message = sent_message_result.unwrap();
            // We need to write this message to the translations map, so we know
            // the language that we came from (and thus will know what language
            // to return to).
            {
                let mut translations = translations_lock.write().await;
                let translation = PastTranslation {
                    language: translation.detected_source_language.clone(),
                    channel_id: msg.channel_id,
                    message_id: msg.id
                };
                translations.entry(sent_message.id.clone()).or_insert(translation.clone());
                println!("Stored the message {:?} with key {}", translation.clone(), sent_message.id.clone());
            };
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

// Actually do the translation HTTP request to DeepL
pub async fn translate_message (msg: String, language_code: String, api_key: &String) -> Translation {

    // Construct the body of the request
    let form_data = [("text", msg.clone()), ("target_lang", language_code.clone())];

    // Do the response with some very ugly chaining until we get the result.
    // TODO: Handle these errors gracefully.
    let response = reqwest::Client::new()
        .post(format!("https://api-free.deepl.com/v2/translate?auth_key={}", api_key)) // <- Create request builder
        .header("User-Agent", "Actix-web")
        .form(&form_data)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    // DeepL gives us back a vector of possible translations, depending on the
    // language that it thinks the message is written in. We only care about
    // returning the first one.
    println!("Posted message \"{}\" to DeepL with target language {} and got back {}", msg.clone(), language_code.clone(), &response.clone());
    let translated_message: DeepLResponse = from_str(&response).unwrap();
    let first_translation = translated_message.translations.first().unwrap();
    if first_translation.detected_source_language == language_code.clone() {
        return Translation { text: String::from(""), detected_source_language: language_code }
    }
    first_translation.clone()

}

fn is_bot_message(bot_id: UserId, message_author_id: UserId) -> bool {
    bot_id == message_author_id
}

fn is_monitored_channel(agg_channel_id: ChannelId, source_channel_list: HashMap<ChannelId, String>, msg_channel_id: ChannelId) -> bool {
    msg_channel_id == agg_channel_id || source_channel_list.contains_key(&msg_channel_id)
}

fn get_replied_to_message(msg: Option<Box<Message>>) -> ReplyTo {
    match msg {
        Some(m) => {
            let msg = *m.clone();
            ReplyTo {
                message: Option::Some(msg.clone()),
                message_id: msg.clone().id,
                author_id: msg.clone().author.id
            }
        },
        None => ReplyTo {
            message: Option::None,
            message_id: MessageId::from(0),
            author_id: UserId::from(0)
        }
    }
}

#[actix_rt::main]
async fn main() {

    let mut app_config: AppConfig = Default::default();
    let mut settings = config::Config::default();
    settings.merge(config::File::with_name("Settings")).unwrap();
    let bot_token = settings.get_str("bot_token").unwrap();
    let bot_user_id: u64 = settings.get("bot_user_id").unwrap();
    let deepl_api_key = settings.get_str("deepl_api_key").unwrap();
    let default_language = settings.get_str("default_language").unwrap();
    let aggregate_channel_id: u64 = settings.get("aggregate_channel_id").unwrap();
    let source_channel_language: HashMap<ChannelId, String> = settings.get("source_channel_language").unwrap();

    app_config.bot_token = bot_token.clone();
    app_config.bot_user_id = UserId::from(bot_user_id);
    app_config.deepl_api_key = deepl_api_key.clone();
    app_config.default_language = default_language.clone();
    app_config.aggregate_channel_id = ChannelId::from(aggregate_channel_id);
    app_config.source_channel_language = source_channel_language.clone();

    println!("App's config: {:?}", app_config);

    // Make an authenticated http client to use with Serenity
    let http_client = serenity::http::client::Http::new_with_token(&app_config.bot_token);

    // Instantiate Serenity with the bot token
    let mut discord_client = ClientBuilder::new_with_http(http_client)
        .token(bot_token)
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    // Now we open a write lock on our data store, so that we can insert some
    // default data into it. We wrap this in a block to ensure the lock is
    // closed immediately after we're done with it.
    {
        let mut data = discord_client.data.write().await;

        // The Translation Value has the following type:
        // Arc<RwLock<HashMap<MessageId, String>>>
        // So, we have to insert the same type to it.
        data.insert::<Translations>(Arc::new(RwLock::new(HashMap::default())));
        data.insert::<AppConfig>(Arc::new(app_config));
    }

    // Start listening for events by starting a single shard of Serenity
    if let Err(why) = discord_client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }

    println!("Babelfish is listening")
}
