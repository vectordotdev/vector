use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

fn main() {
    println!("highlighter!");

    tree_sitter_vrl::HIGHLIGHTS_QUERY;

    let highlight_names = tree_sitter::Query::new(
        tree_sitter_vrl::language(),
        tree_sitter_vrl::HIGHLIGHTS_QUERY,
    )
    .unwrap()
    .capture_names()
    .into_iter()
    .cloned()
    .collect::<Vec<_>>();

    let mut highlighter = Highlighter::new();

    let mut vrl_config = HighlightConfiguration::new(
        tree_sitter_vrl::language(),
        tree_sitter_vrl::HIGHLIGHTS_QUERY,
        "",
        "",
    )
    .unwrap();
    vrl_config.configure(&highlight_names);

    let source_code = "[true, \"test\", false]";

    let highlights = highlighter
        .highlight(&vrl_config, source_code.as_bytes(), None, |_| None)
        .unwrap();

    let mut highlighted_code = String::with_capacity(source_code.len() * 2);
    let mut tags = vec![];

    for event in highlights {
        match event.unwrap() {
            HighlightEvent::Source { start, end } => {
                let text = &source_code[start..end];
                let classes = tags.join(" ");
                let item = format!("<span class=\"{}\">{}</span>", classes, text);
                highlighted_code += &item;
            }
            HighlightEvent::HighlightStart(s) => {
                tags.push(highlight_names[s.0].clone());
            }
            HighlightEvent::HighlightEnd => {
                tags.pop();
            }
        }
    }

    println!("{}", highlighted_code);
}
