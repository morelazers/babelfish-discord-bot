# Babelfish

Babelfish is a Discord bot who can help you communicate with an international community.

## Usage

Install Babelfish on your Discord server with the following link: https://discord.com/api/oauth2/authorize?client_id=851154413207814204&permissions=11264&scope=bot

By default, Babelfish is listening in channels with names of the form: `intl-XX`, where `XX` is a two-letter country code supported by DeepL: https://www.deepl.com/docs-api/other-functions/listing-supported-languages/.

It will translate messages in these channel into English, because I made the bot and I speak English.

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
