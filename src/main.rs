use std::{
    env,
    time::{SystemTime, UNIX_EPOCH},
};

use cosmic_text::{FontSystem, SwashCache};
use futures::StreamExt;
use rand::{distributions, thread_rng, Rng};
use sqlx::{query, SqlitePool};
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};
use telegram_bot::*;

mod render;

struct State {
    pub api: Api,
    pub pool: SqlitePool,
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub syntax_set: SyntaxSet,
    pub theme_set: ThemeSet,
    pub config: Config,
}

struct Config {
    null_chat_id: i64,
}

const UNUSED_RESULT_ID: &str = "unused-result-id";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[allow(unused_must_use)]
    {
        dotenvy::dotenv();
    }
    env_logger::init();

    let token = env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN not set");
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
    let api = Api::new(token);
    let pool = sqlx::SqlitePool::connect(&database_url).await?;
    let font_system = FontSystem::new();
    let swash_cache = SwashCache::new();
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let config = Config {
        null_chat_id: -992674722,
    }; // TODO: accept this from outside
    let mut state = State {
        api,
        pool,
        font_system,
        swash_cache,
        config,
        syntax_set,
        theme_set,
    };

    sqlx::migrate!().run(&state.pool).await?;

    // Fetch new updates via long poll method
    let mut stream = state.api.stream();
    while let Some(update) = stream.next().await {
        let update = update?;
        if let Err(e) = process_update(update, &mut state).await {
            log::error!("Error while processing update: {e}");
        }
    }
    Ok(())
}

async fn process_update(update: Update, state: &mut State) -> anyhow::Result<()> {
    // If the received update contains a new message...
    match update.kind {
        UpdateKind::Message(message) => {
            if let MessageKind::Text { ref data, .. } = message.kind {
                // Print received text message to stdout.
                println!("<{}>: {}", &message.from.first_name, data);

                // Answer message with "Hi".
                state
                    .api
                    .send(message.text_reply(format!(
                        "Hi, {}! You just wrote '{}' in chat {}",
                        &message.from.first_name,
                        data,
                        message.chat.id()
                    )))
                    .await?;
            }
        }
        UpdateKind::InlineQuery(query) => {
            process_inline_query(state, query).await?;
        }
        UpdateKind::ChosenInlineResult(chosen_inline_result) => {
            if chosen_inline_result.result_id == UNUSED_RESULT_ID {
                return Ok(());
            }
            // Once an inline result has been chosen, we need to make the gist corresponding to it not ephemeral.
            query!(
                "UPDATE gist SET is_ephemeral=0 WHERE id=?",
                chosen_inline_result.result_id
            )
            .execute(&state.pool)
            .await?;

            // Also remove the expired ephemeral gists
            let delete_cutoff = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
                - 1800; // 30 minutes
            query!(
                "DELETE FROM gist WHERE is_ephemeral>0 AND sent_at_unix_time<?",
                delete_cutoff
            )
            .execute(&state.pool)
            .await?;
        }
        _ => {}
    }

    Ok(())
}

/// Process an inline query: save the code snippet as an ephemeral gist,
/// then render it as an image
/// and submit this as an inline query result.
async fn process_inline_query(state: &mut State, inline_query: InlineQuery) -> anyhow::Result<()> {
    let api = &state.api;
    let pool = &state.pool;
    let font_system = &mut state.font_system;
    let swash_cache = &mut state.swash_cache;
    let ps = &state.syntax_set;
    let ts = &state.theme_set;
    let config = &state.config;

    let code = inline_query.query.clone();
    let mut real_code = None;
    let mut code_ext = None;
    // If there is a single word in front of the first colon, that's considered the file extension
    if code.find(":").is_some() {
        let first = code.split(":").nth(0).unwrap();
        if first.find(" ").is_none() {
            code_ext = Some(first.clone());
            real_code = Some(code.split(":").skip(1).collect::<Vec<&str>>().join(":"));
        }
    }

    let code = if let Some(real_code) = real_code {
        real_code
    } else {
        code.clone()
    };

    // If the user did not type any code yet, then show an invitation to type some code, and do not render the empty code.
    if code.trim().is_empty() {
        api.send(
            inline_query.answer(vec![InlineQueryResult::InlineQueryResultArticle(
                InlineQueryResultArticle {
                    id: UNUSED_RESULT_ID.to_string(),
                    title: "Type some code to highlight!".to_string(),
                    description: Some("try: py:print('Hello World')".to_string()),
                    input_message_content: InputMessageContent::InputTextMessageContent(
                        InputTextMessageContent {
                            message_text: r"Welcome to the code highlighter bot\! To highlight some code, type it in the inline query box\. For example: `py:print\('Hello World'\)` and `cpp:int main\(int argc, char **argv\);`".to_string(),
                            parse_mode: Some(ParseMode::MarkdownV2),
                            disable_web_page_preview: true,
                        },
                    ),
                    reply_markup: None,
                    url: None,
                    hide_url: true,
                    thumb_url: None,
                    thumb_width: None,
                    thumb_height: None,
                },
            )]),
        )
        .await?;

        return Ok(());
    }

    let syntax = if let Some(ext) = code_ext {
        ps.find_syntax_by_extension(ext)
    } else {
        Some(ps.find_syntax_plain_text())
    };

    // Keep making IDs until an insertion succeeds, up to a maximum of 100 attempts
    let mut attempts = 0;
    let mut rand = thread_rng();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let mut id = String::with_capacity(8); // 8 character IDs
    let from: i64 = inline_query.from.id.into();
    loop {
        id.clear();
        for _ in 0..8 {
            id.push(rand.sample(distributions::Alphanumeric) as char);
        }
        let result = query!("INSERT INTO gist (id, content, sent_by, sent_at_unix_time, is_ephemeral, language) VALUES (?, ?, ?, ?, 1, ?)",
            id, code, from, now, code_ext).execute(pool).await;
        if result.is_err() {
            eprintln!("Error while inserting gist: {result:?}");
            attempts += 1;
            if attempts > 100 {
                return Err(result.unwrap_err())?;
            }
        } else {
            break;
        }
    }

    // In order to attach a photo, it needs to first be uploaded to some chat, which is specified by config.null_chat_id.
    // Set this to a chat that you control.
    // This will yield a server file_id, which can be then used in the inline query result photo.

    let png_data = render::draw_code(
        font_system,
        swash_cache,
        ps,
        ts,
        &code,
        syntax.unwrap_or(ps.find_syntax_plain_text()),
    );

    let photo_upload = InputFileUpload::with_data(png_data, "code.png");

    let upload = api
        .send(SendPhoto::new(
            ChatId::from(config.null_chat_id),
            photo_upload,
        ))
        .await?;
    let file_id = if let MessageKind::Photo { data, .. } = upload.kind {
        let first = data[0].clone();
        let largest = data.iter().fold(first, |acc, item| {
            if (acc.width, acc.height) < (item.width, item.height) {
                item.clone()
            } else {
                acc
            }
        });
        largest.file_id
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Uploaded image with photo but it wasn't a photo message?!"),
        ))?;
    };

    let language = if code_ext.is_some() {
        format!(
            "Language: `{}`{}",
            code_ext.unwrap(),
            if let None = syntax {
                r" \(ERROR: could not find syntax by this extension\!\)"
            } else {
                ""
            }
        )
    } else {
        format!(
            r"Language unknown \(try `py:print\('Hello World'\)` and `cpp:int main\(int argc, char **argv\);`\)"
        )
    };

    api.send(
        inline_query.answer(vec![InlineQueryResult::InlineQueryResultCachedPhoto(
            InlineQueryResultCachedPhoto {
                id: id.clone(),
                photo_file_id: file_id,
                title: None,
                description: None,
                caption: Some(format!("Code snippet ID: `{id}` {}", language)),
                parse_mode: Some(ParseMode::MarkdownV2),
                reply_markup: None,
                input_message_content: None,
            },
        )]),
    )
    .await?;
    Ok(())
}
