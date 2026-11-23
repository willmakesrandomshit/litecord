#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageSegment {
    Text(String),
    InlineCode(String),
    CodeBlock(String),
    Spoiler(String),
    Mention(String),
}

/// Parses Discord-style code blocks (```...```), inline code (`...`),
/// spoilers (||...||), and @mentions into structured segments.
pub fn parse_message_segments(input: &str) -> Vec<MessageSegment> {
    let mut out = Vec::new();
    let mut rest = input;

    while !rest.is_empty() {
        if let Some(after_open) = rest.strip_prefix("```") {
            if let Some(close_idx) = after_open.find("```") {
                let block = after_open[..close_idx].trim();
                if !block.is_empty() {
                    out.push(MessageSegment::CodeBlock(block.to_string()));
                }
                rest = &after_open[close_idx + 3..];
                continue;
            }
        }

        if let Some(after_open) = rest.strip_prefix("||") {
            if let Some(close_idx) = after_open.find("||") {
                if close_idx > 0 {
                    out.push(MessageSegment::Spoiler(after_open[..close_idx].to_string()));
                    rest = &after_open[close_idx + 2..];
                    continue;
                }
            }
        }

        if let Some(after_open) = rest.strip_prefix('`') {
            if let Some(close_idx) = after_open.find('`') {
                if close_idx > 0 {
                    out.push(MessageSegment::InlineCode(
                        after_open[..close_idx].to_string(),
                    ));
                    rest = &after_open[close_idx + 1..];
                    continue;
                }
            }
        }

        if rest.starts_with('@') {
            let end = rest[1..]
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map(|i| i + 1)
                .unwrap_or(rest.len());
            if end > 1 {
                out.push(MessageSegment::Mention(rest[..end].to_string()));
                rest = &rest[end..];
                continue;
            }
        }

        // Consume plain text until next special token (UTF-8 safe).
        let skip = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        let tail = &rest[skip..];
        let next_special = [
            tail.find("```").map(|i| i + skip),
            tail.find("||").map(|i| i + skip),
            tail.find('`').map(|i| i + skip),
            tail.find('@').map(|i| i + skip),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(rest.len());

        let chunk = &rest[..next_special];
        if let Some(MessageSegment::Text(prev)) = out.last_mut() {
            prev.push_str(chunk);
        } else {
            out.push(MessageSegment::Text(chunk.to_string()));
        }
        rest = &rest[next_special..];
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_code_spoilers_and_mentions() {
        let segs = parse_message_segments(
            "Hey @ferris_dev check `cargo test` and ||secret|| and ```rust\nfn main() {}\n```",
        );
        assert!(segs.contains(&MessageSegment::Mention("@ferris_dev".into())));
        assert!(segs.contains(&MessageSegment::InlineCode("cargo test".into())));
        assert!(segs.contains(&MessageSegment::Spoiler("secret".into())));
        assert!(segs.contains(&MessageSegment::CodeBlock("rust\nfn main() {}".into())));
    }

    #[test]
    fn parses_multibyte_utf8_and_emojis_without_panicking() {
        let segs = parse_message_segments("🚀🔥 café @user `code` ||spoiler||");
        assert!(!segs.is_empty());
    }
}
