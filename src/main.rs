use std::env;

use futures::StreamExt;
use telegram_bot::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let token = env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN not set");
    let api = Api::new(token);

    // Fetch new updates via long poll method
    let mut stream = api.stream();
    while let Some(update) = stream.next().await {
        // If the received update contains a new message...
        let update = update?;
        match update.kind {
            UpdateKind::Message(message) => {
                if let MessageKind::Text { ref data, .. } = message.kind {
                    // Print received text message to stdout.
                    println!("<{}>: {}", &message.from.first_name, data);
    
                    // Answer message with "Hi".
                    api.send(message.text_reply(format!(
                        "Hi, {}! You just wrote '{}'",
                        &message.from.first_name, data
                    )))
                    .await?;
                }    
            },
            UpdateKind::InlineQuery(query) => {
                println!("{query:?}");
                let mut kb = InlineKeyboardMarkup::new();
                let mut row = kb.add_empty_row();
                row.push(InlineKeyboardButton::callback("Save as gist".to_string(), "save_as_gist".to_string()));

                api.send(query.answer(vec![
                    InlineQueryResult::InlineQueryResultArticle(
                    InlineQueryResultArticle {
                        id: "what".to_string(), title: "What".to_string(),
                        input_message_content: InputMessageContent::InputTextMessageContent(
                            InputTextMessageContent { message_text: "Hello".to_string(), parse_mode: None, disable_web_page_preview: false }
                        ),
                        reply_markup: Some(kb),
                        url: Some("https://example.com".to_string()),
                        hide_url: false,
                        description: None,
                        thumb_url: None,
                        thumb_width: None,
                        thumb_height: None
                    })
                ])).await?;
            },
            UpdateKind::ChosenInlineResult(chosen_inline_result) => {
                println!("{chosen_inline_result:?}");
            },
            _ => {},
        }

    }
    Ok(())
}