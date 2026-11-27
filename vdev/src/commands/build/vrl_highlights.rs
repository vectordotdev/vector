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
                r#"(?s)(<code\b[^>]*\bclass=(?:"language-vrl"|'language-vrl'|language-vrl\b)[^>]*>)(.*?)(</code>)"#,
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
            output.push_str(block[1].trim_end_matches('>'));
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
    fn highlights_vrl_program() {
        let mut renderer = Renderer::new().unwrap();
        let source = "value = parse_json!(.message)
.result = value
.processed = true
";
        let input = format!(
            r#"<code class="language-vrl" data-lang="vrl">{}</code>"#,
            html_escape::encode_text(source)
        );
        let (output, count) = renderer.page(&input).unwrap();
        assert_eq!(count, 1);
        for token in [
            "<span class=\"vrl-function\">parse_json</span>",
            "<span class=\"vrl-property\">message</span>",
            "<span class=\"vrl-variable\">value</span>",
            "<span class=\"vrl-boolean\">true</span>",
        ] {
            assert!(output.contains(token), "Missing {token} in {output}");
        }
        let code = renderer.blocks.captures(&output).unwrap();
        let text = renderer.spans.replace_all(&code[2], "");
        assert_eq!(html_escape::decode_html_entities(&text), source);
    }
}
