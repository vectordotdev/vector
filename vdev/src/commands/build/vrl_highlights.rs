use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use regex::Regex;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

use crate::utils::paths::{find_repo_root, resolve_repo_relative_path};

unsafe extern "C" {
    fn tree_sitter_vrl() -> *const ();
}

// SAFETY: The bundled generated parser exports this function with Tree-sitter's
// language ABI and returns a pointer to its static language definition.
const LANGUAGE: tree_sitter_language::LanguageFn =
    unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_vrl) };

const CAPTURES: &[&str] = &[
    "comment",
    "constant",
    "boolean",
    "number",
    "string",
    "string.escape",
    "string.regexp",
    "function",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "property",
    "keyword",
    "operator",
    "punctuation",
];

/// Highlight VRL in a rendered Hugo site using the Tree-sitter VRL grammar.
#[derive(clap::Args, Debug)]
pub struct Cli {
    /// Rendered website directory, relative to the repository root.
    #[arg(long, default_value = "website/public")]
    site_dir: PathBuf,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let root = find_repo_root()?;
        let site = resolve_repo_relative_path(&root, &self.site_dir, "Rendered website")?;
        ensure!(site.is_dir(), "Build the website first: {}", site.display());
        let mut renderer = Renderer::new()?;
        let mut blocks = 0;
        let mut pages = 0;
        for entry in glob::glob(&format!(
            "{}/**/*.html",
            glob::Pattern::escape(&site.to_string_lossy())
        ))? {
            let path = entry?;
            if path.is_symlink() {
                continue;
            }
            let html = fs::read_to_string(&path)?;
            let (highlighted_page, count) = renderer
                .page(&html)
                .with_context(|| format!("Failed to highlight {}", path.display()))?;
            if count > 0 {
                fs::write(&path, highlighted_page)?;
                blocks += count;
                pages += 1;
            }
        }
        info!("Highlighted {blocks} VRL snippets across {pages} pages");
        Ok(())
    }
}

struct Renderer {
    config: HighlightConfiguration,
    highlighter: Highlighter,
    blocks: Regex,
    spans: Regex,
    line_numbers: Regex,
    cache: HashMap<String, String>,
}

impl Renderer {
    fn new() -> Result<Self> {
        let mut config = HighlightConfiguration::new(
            LANGUAGE.into(),
            "vrl",
            include_str!("vrl_highlights/highlights.scm"),
            "",
            "",
        )?;
        config.configure(CAPTURES);
        Ok(Self {
            config,
            highlighter: Highlighter::new(),
            // Hugo emits these code elements with escaped source and span-only token markup.
            blocks: Regex::new(
                r#"(?s)(<code\b[^>]*\bclass=(?:"language-(?:coffee|vrl)"|'language-(?:coffee|vrl)'|language-(?:coffee|vrl)\b)[^>]*>)(.*?)(</code>)"#,
            )?,
            spans: Regex::new(r"</?span\b[^>]*>")?,
            line_numbers: Regex::new(
                r#"(?s)<span class=(?:"(?:ln|lnt)"|'(?:ln|lnt)'|(?:ln|lnt)\b)[^>]*>.*?</span>"#,
            )?,
            cache: HashMap::new(),
        })
    }

    fn page(&mut self, html: &str) -> Result<(String, usize)> {
        let mut output = String::with_capacity(html.len());
        let mut offset = 0;
        let mut count = 0;
        // Collect first so highlighting can reuse the mutable parser and snippet cache.
        let blocks: Vec<_> = self.blocks.captures_iter(html).collect();
        for block in blocks {
            let matched = block.get(0).expect("matched code block");
            output.push_str(
                html.get(offset..matched.start())
                    .context("Invalid HTML match boundary")?,
            );
            offset = matched.end();
            if block[1].contains("data-highlighter=") {
                output.push_str(matched.as_str());
                continue;
            }

            let numbers: Vec<_> = self
                .line_numbers
                .find_iter(&block[2])
                .map(|number| number.as_str().to_owned())
                .collect();
            let without_numbers = self.line_numbers.replace_all(&block[2], "");
            let text = self.spans.replace_all(&without_numbers, "");
            let source = html_escape::decode_html_entities(&text);
            let highlighted = self.highlight(&source)?;
            let opening = block[1]
                .replace("language-coffee", "language-vrl")
                .replace("data-lang=\"coffee\"", "data-lang=\"vrl\"")
                .replace("data-lang='coffee'", "data-lang='vrl'")
                .replace("data-lang=coffee", "data-lang=vrl");
            output.push_str(opening.trim_end_matches('>'));
            output.push_str(" data-highlighter=\"tree-sitter-vrl\">");
            if numbers.is_empty() {
                output.push_str(&highlighted);
            } else {
                // Retain inline line-number elements, which copy controls deliberately exclude.
                for (index, line) in highlighted.split_inclusive('\n').enumerate() {
                    if let Some(number) = numbers.get(index) {
                        output.push_str(number);
                    }
                    output.push_str(line);
                }
            }
            output.push_str(&block[3]);
            count += 1;
        }
        output.push_str(
            html.get(offset..)
                .context("Invalid HTML remainder boundary")?,
        );
        Ok((output, count))
    }

    fn highlight(&mut self, source: &str) -> Result<String> {
        if let Some(html) = self.cache.get(source) {
            return Ok(html.clone());
        }
        let events = self
            .highlighter
            .highlight(&self.config, source.as_bytes(), None, |_| None)?;
        let mut html = String::new();
        for event in events {
            match event? {
                HighlightEvent::Source { start, end } => {
                    let text = source
                        .get(start..end)
                        .context("Invalid Tree-sitter UTF-8 boundary")?;
                    html.push_str(&html_escape::encode_text(text));
                }
                HighlightEvent::HighlightStart(highlight) => {
                    html.push_str("<span class=\"vrl-");
                    html.push_str(&CAPTURES[highlight.0].replace('.', "-"));
                    html.push_str("\">");
                }
                HighlightEvent::HighlightEnd => html.push_str("</span>"),
            }
        }
        self.cache.insert(source.to_owned(), html.clone());
        Ok(html)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_vrl_functions_fields_variables_and_literals() {
        let mut renderer = Renderer::new().unwrap();
        let html = renderer.highlight("value = parse_json!(.message)\n.result = value\n.kind = r'\\w+'\nif true { abort }\n").unwrap();
        for token in [
            "<span class=\"vrl-function\">parse_json</span>",
            "<span class=\"vrl-property\">message</span>",
            "<span class=\"vrl-variable\">value</span>",
            "<span class=\"vrl-boolean\">true</span>",
            "<span class=\"vrl-keyword\">if</span>",
            "vrl-string-regexp",
        ] {
            assert!(html.contains(token), "Missing {token} in {html}");
        }
    }

    #[test]
    fn preserves_source_escapes_html_and_is_idempotent() {
        let mut renderer = Renderer::new().unwrap();
        let source = ".message = \"<script>é & 🦀</script>\"\n# comment\n";
        let input = format!(
            "<div class=\"highlight\" title=\"example.vrl\"><pre class=\"chroma\"><code class=\"language-vrl\" data-lang=\"vrl\">{}</code></pre></div>",
            html_escape::encode_text(source)
        );
        let (output, count) = renderer.page(&input).unwrap();
        assert_eq!(count, 1);
        assert!(output.contains("title=\"example.vrl\""));
        assert!(!output.contains("<script>"));
        let code = renderer.blocks.captures(&output).unwrap();
        let text = renderer.spans.replace_all(&code[2], "");
        assert_eq!(html_escape::decode_html_entities(&text), source);
        assert_eq!(renderer.page(&output).unwrap(), (output, 0));
    }

    #[test]
    fn preserves_inline_numbers_and_leaves_other_languages_alone() {
        let mut renderer = Renderer::new().unwrap();
        let input = "<code class=\"language-coffee\" data-lang=\"coffee\"><span class=\"line\"><span class=\"ln\">1</span><span class=\"cl\">.x = 1\n</span></span></code><code class=\"language-yaml\">source: test</code>";
        let (output, count) = renderer.page(input).unwrap();
        assert_eq!(count, 1);
        assert!(output.contains("<span class=\"ln\">1</span>"));
        assert!(output.contains("<code class=\"language-yaml\">source: test</code>"));
        assert!(output.contains("language-vrl"));
    }

    #[test]
    fn handles_minified_hugo_output() {
        let mut renderer = Renderer::new().unwrap();
        let input = "<code class=language-coffee data-lang=coffee><span class=line><span class=ln>1</span><span class=cl>.x = 1\n</span></span></code>";
        let (output, count) = renderer.page(input).unwrap();
        assert_eq!(count, 1);
        assert!(output.contains("class=language-vrl data-lang=vrl"));
        assert!(output.contains("<span class=ln>1</span>"));
        assert!(output.contains("vrl-property"));
    }
}
