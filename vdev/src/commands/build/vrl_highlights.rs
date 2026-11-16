use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use regex::Regex;
use syntect::{
    html::{ClassStyle, ClassedHTMLGenerator},
    parsing::{SyntaxDefinition, SyntaxSet, SyntaxSetBuilder},
    util::LinesWithEndings,
};

use crate::utils::paths::{find_repo_root, resolve_repo_relative_path};

/// Highlight VRL in a rendered Hugo site using a pure Rust Syntect syntax definition.
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
    syntaxes: SyntaxSet,
    blocks: Regex,
    spans: Regex,
    line_numbers: Regex,
    cache: HashMap<String, String>,
}

impl Renderer {
    fn new() -> Result<Self> {
        let mut syntaxes = SyntaxSetBuilder::new();
        syntaxes.add(SyntaxDefinition::load_from_str(
            include_str!("vrl_highlights/VRL.sublime-syntax"),
            true,
            None,
        )?);
        Ok(Self {
            syntaxes: syntaxes.build(),
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
            output.push_str(" data-highlighter=\"syntect-vrl\">");
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
        let syntax = self
            .syntaxes
            .find_syntax_by_extension("vrl")
            .context("Missing VRL syntax")?;
        let mut generator = ClassedHTMLGenerator::new_with_class_style(
            syntax,
            &self.syntaxes,
            ClassStyle::SpacedPrefixed { prefix: "vrl-" },
        );
        for line in LinesWithEndings::from(source) {
            generator.parse_html_for_line_which_includes_newline(line)?;
        }
        let html = generator.finalize();
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
    fn keeps_function_like_text_in_strings_regexes_and_comments_unhighlighted() {
        let mut renderer = Renderer::new().unwrap();
        let source = r#".result = parse_json!(.message)
.text = "parse_json(.fake) # literal \" quote"
.raw = s'parse_json(.fake)'
.pattern = r'parse_json\(.fake\)'
# parse_json(.fake)
"#;
        let html = renderer.highlight(source).unwrap();
        assert_eq!(html.matches("class=\"vrl-function\"").count(), 1);
        assert!(!html.contains("class=\"vrl-property\">fake"));
        assert!(html.contains("class=\"vrl-comment\""));
        assert!(html.contains("class=\"vrl-string-escape\""));
        let text = renderer.spans.replace_all(&html, "");
        assert_eq!(html_escape::decode_html_entities(&text), source);
    }

    #[test]
    fn highlights_interpolation_and_resumes_the_enclosing_string() {
        let mut renderer = Renderer::new().unwrap();
        let source =
            ".text = \"Hello {{ upcase(.name) }} # literal\"\n.when = t'2026-11-16T12:00:00Z'\n";
        let html = renderer.highlight(source).unwrap();
        assert!(html.contains("class=\"vrl-function\">upcase</span>"));
        assert!(html.contains("class=\"vrl-property\">name</span>"));
        assert!(!html.contains("class=\"vrl-comment\""));
        assert!(html.contains("class=\"vrl-constant\""));
        let text = renderer.spans.replace_all(&html, "");
        assert_eq!(html_escape::decode_html_entities(&text), source);
    }

    #[test]
    fn highlights_vrl_numeric_separators_and_at_prefixed_fields() {
        let mut renderer = Renderer::new().unwrap();
        let html = renderer
            .highlight(".@timestamp = 1_000\n.value = 1_2_3._4_0_\n")
            .unwrap();
        assert!(html.contains("class=\"vrl-property\">@timestamp</span>"));
        assert!(html.contains("class=\"vrl-number\">1_000</span>"));
        assert!(html.contains("class=\"vrl-number\">1_2_3._4_0_</span>"));
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
