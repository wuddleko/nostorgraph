const POST_CSS: &str = include_str!("../../templates/post.css");

pub(super) fn render(note: &nostorgraph_nip44::Note) -> Result<String, String> {
    let config = crate::config::Config::load()?;
    let options = crate::render_options(&config);
    let body = nostorgraph_parser::render_markdown_with_config(&note.content, &options);
    let title = nostorgraph_parser::html_escape(&note.title);
    let byline = if note.author.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div class="article-meta" id="sealed-author">{}</div>"#,
            nostorgraph_parser::html_escape(&note.author)
        )
    };
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
{css}
</style>
</head>
<body>
<div class="container">
<div class="content">
<h1 id="sealed-title">{title}</h1>
<svg class="wave" viewBox="0 0 640 16" preserveAspectRatio="none" aria-hidden="true"><path d="M0 8 H640"></path></svg>
{byline}
<div class="article-content" id="CONTENT">
{body}
</div>
</div>
</div>
</body>
</html>
"#,
        css = POST_CSS,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_document_shows_the_title_and_the_phrase_without_the_key() {
        let note = nostorgraph_nip44::Note {
            title: "Opened note".to_string(),
            author: "Ada".to_string(),
            content: "See **violet ink** here.".to_string(),
            locator: "0123456789abcdef0123".to_string(),
        };
        let key = "ntgraph-secret-key-value";
        let html = render(&note).expect("render");
        assert!(html.contains("<title>Opened note</title>"));
        assert!(html.contains("<h1 id=\"sealed-title\">Opened note</h1>"));
        assert!(html.contains("id=\"CONTENT\""));
        assert!(html.contains("id=\"sealed-author\""));
        assert!(html.contains("<strong>violet ink</strong>"));
        assert!(html.contains("Ada"));
        assert!(!html.contains(key));
        assert!(html.contains(".article-content"));
        assert!(!html.contains("/post.css"));
    }
}
