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
    let _o = ngrammatic::CorpusBuilder::new()
        .arity(2)
        .pad_full(ngrammatic::Pad::Auto)
        .finish()
        .search("ook", 0.25)
        .first();

    display(&diag());
}
