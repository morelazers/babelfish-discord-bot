// Data Storage
use std::{collections::HashMap, sync::Arc};

// Useful http things
use serde::Deserialize;
use serde_json::{from_str};
use reqwest;

// Discord Client
use serenity::{
    async_trait,
    client::{Context, EventHandler, ClientBuilder},
    model::{channel::{Message, MessageReference},id::MessageId, gateway::Ready},
    prelude::{RwLock,TypeMapKey}
};

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

// A map of MessageId => String
struct Translations;

// The thing we're storing is a rw-locked HashMap. wrapped in an Arc
impl TypeMapKey for Translations {
    type Value = Arc<RwLock<HashMap<MessageId, String>>>;
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {

        // Don't reply to messages from self
        // TODO: Factor this out into config?
        if msg.author.id == 851154413207814204 {
            return
        }

        // Check that our Channel's name starts with "intl-"
        // TODO: Factor this out into config?
        let channel = msg.channel_id.to_channel(&ctx.http).await.unwrap();
        let channel_name = &channel.clone().guild().unwrap().name;
        if !channel_name.contains("intl-") {
            return
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

        // Get the channel's language
        // TODO: Maybe store this in a map too? Though it is quite easy here..
        let channel_lang = &channel_name[5..].to_uppercase();

        // Colonialism in action
        let default_lang = String::from("EN");

        // We might not want to translate into the channel's language though. If
        // we are replying to someone whose message wss subsequently translated
        // (because they are talking in the channel's language), we should
        // translate into the language of the replied-to message
        let mut target_lang = String::from(default_lang);

        // Get a reference to the replied-to message
        let reply_to = match msg.referenced_message.clone() {
            Some(m) => m.id,
            None => MessageId::from(0)
        };

        // Now we need to find out if the replied-to message has been translated
        // already. If it has, we'll translate back to its source language.
        // To do this, we need to activate our read lock on the data, then use
        // it to overwrite the default target language which was derived from
        // the channel name
        {
            let past_translations = translations_lock
               .read()
               .await;

            let replying_to_source_lang = past_translations
                .get(&reply_to);

            target_lang = match replying_to_source_lang {
                Some(s) => s.clone(),
                None => target_lang
            };
        };

        println!("Got reply to {}, translating to {}", &reply_to, &target_lang);

        // Go do the translation with deepL
        let translation = translate_message(msg.content.clone(), String::from(&target_lang)).await;


        // Now write this message's id to storage, keying its source language
        {
            let mut translations = translations_lock.write().await;
            let source_lang = translation.detected_source_language.clone().to_uppercase();
            translations.entry(msg.id.clone()).or_insert(channel_lang.clone());
        };


        // No translation necessary - already source language
        if translation.text.eq("") || translation.text == msg.content {
            return
        }

        if let Err(why) = msg.channel_id.send_message(&ctx.http, |f| {
            let mut msg_ref = MessageReference::from(&msg.clone());

            // Here lies some code which conditionally attaches  the bot's reply
            // to either the just-translated message, or the message which was
            // replied-to by said message (whose language we have just
            // translated back into).
            // Not sure whether this is too confusing or not though, and you
            // loee the "audit trail"
            if reply_to != 0 {
                msg_ref = MessageReference::from((channel.id().clone(), reply_to));
            }

            f.reference_message(msg_ref).content(translation.text)
        }).await {
            println!("Error sending message: {:?}", why);
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

// Actually do the translation HTTP request to DeepL
pub async fn translate_message (msg: String, language_code: String) -> Translation {

    // Construct the body of the request
    let form_data = [("text", msg), ("target_lang", language_code.clone())];

    // Do the response with some very ugly chaining until we get the result.
    // TODO: Handle these errors gracefully.
    let response = reqwest::Client::new()
        .post("https://api-free.deepl.com/v2/translate?auth_key=DEEPL_API_KEY") // <- Create request builder
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
    let translated_message: DeepLResponse = from_str(&response).unwrap();
    let first_translation = translated_message.translations.first().unwrap();
    if first_translation.detected_source_language == language_code.clone() {
        return Translation { text: String::from(""), detected_source_language: language_code }
    }
    first_translation.clone()

}

#[actix_rt::main]
async fn main() {

    // TODO: Put this in a config file
    let bot_token = "DISCORD_BOT_TOKEN";

    // Make an authenticated http client to use with Serenity
    let http_client = serenity::http::client::Http::new_with_token(bot_token);

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
    }

    // Start listening for events by starting a single shard of Serenity
    if let Err(why) = discord_client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }

    println!("Babelfish is listening")
}
