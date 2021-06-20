# Babelfish (unstable)

Babelfish is a Discord bot who can help you communicate with an international community.

## Prerequisites

- Discord Bot
- DeepL API Key

## Usage

Install Babelfish on your Discord server via the OAuth link that appears when you select the "bot" OAuth2 scope for your application.

Clone the repo and replace `DEEPL_API_KEY` and `DISCORD_BOT_TOKEN` with your own values.

Build the repo with `cargo build --release`

Run the repo with `./target/release/bot`

By default, Babelfish is listening in channels with names of the form: `intl-XX`, where `XX` is a two-letter country code supported by DeepL: https://www.deepl.com/docs-api/other-functions/listing-supported-languages/.

It will translate messages in these channels into English, because I made the bot and I speak English. You can change this by finding the "EN" country code and replacing it with whichever language you like.

## Example

In a channel `intl-de`, the following exchange can occur, allowing Bob and Alicia to converse despite neither of them knowing the language of the other.

```
Alicia: Hallo alles! Ich bin ein guten tage haben.
Babelfish: @alicia: Hello all! I'm having a good day.
Bob: @alicia: Hello Alicia! That's great news.
Babelfish: @alicia: Hallo Alicia! Das sind tolle Neuigkeiten.
```

## Todo

- The storage could be improved, should add ChannelId => lang to avoid saving it over and over
- Not sure about the reply system, you lose the "audit trail" of messages and their translations
- Currently just builds a HashMap in memory and will crash eventually
