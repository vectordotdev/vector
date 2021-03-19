use codespan_reporting::diagnostic;

#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

fn diag() -> diagnostic::Diagnostic<()> {
    diagnostic::Diagnostic {
        severity: diagnostic::Severity::Error,
        code: Some("E001".to_string()),
        message: "ohno".to_string(),
        labels: Vec::new(),
        notes: vec!["see language documentation at https://vrl.dev".to_string()],
    }
}

fn display(diagnostic: &diagnostic::Diagnostic<()>) {
    use codespan_reporting::files::SimpleFile;
    use codespan_reporting::term;
    use termcolor::Buffer;

    let file = SimpleFile::new("", ".ook = matches(s'nork')");
    let mut config = term::Config::default();
    config.display_style = term::DisplayStyle::Short;
    let mut buffer = Buffer::ansi();

    println!("{:?}", diagnostic);

    term::emit(&mut buffer, &config, &file, &diagnostic)
        .map_err(|_| std::fmt::Error)
        .unwrap();

    let string = std::str::from_utf8(buffer.as_slice()).unwrap();
    println!("{:?}", buffer.as_slice());
    println!("{}", string);
}

fn main() {
    /*
    let corpus = ngrammatic::Corpus {
            arity: 2,
            ngrams: std::collections::HashMap::new(),
            pad_left: ngrammatic::Pad::Auto,
            pad_right: ngrammatic::Pad::Auto,
            key_trans: Box::new(|x| x.into()),
        };

    let _o = corpus
        .search("ook", 0.25);
    */

    use std::collections::HashMap;
    let mut grams: HashMap<String, usize> = HashMap::new();

    for window in " ohno ".chars().collect::<Vec<_>>().windows(2) {
        let count = grams.entry(window.iter().collect()).or_insert(0);
        *count += 1;
    }

    display(&diag());
}
