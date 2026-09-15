//! Markdown in, HTML that cannot bite out.

pub fn render(markdown: &str) -> String {
    let mut options = comrak::Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.render.r#unsafe = false;
    let html = comrak::markdown_to_html(markdown, &options);
    ammonia::Builder::default()
        .link_rel(Some("noopener noreferrer"))
        .clean(&html)
        .to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn scripts_and_javascript_links_do_not_survive() {
        let out = super::render("hi <script>x()</script> [a](javascript:alert(1)) *em*");
        assert!(!out.contains("<script"));
        assert!(!out.contains("javascript:"));
        assert!(out.contains("<em>em</em>"));
    }
}
