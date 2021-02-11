use std::fmt;

#[derive(Debug, PartialEq, Clone)]
pub enum Note {
    Hint(String),
    CoerceValue,
    SeeFunctionDocs(&'static str),
    SeeErrorDocs,
    SeeCodeDocs(usize),
    SeeLangDocs,

    #[doc(hidden)]
    SeeDocs(String, String),
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Note::*;

        match self {
            Hint(hint) => write!(f, "hint: {}", hint),
            CoerceValue => {
                Hint("coerce the value using one of the coercion functions".to_owned()).fmt(f)
            }
            SeeFunctionDocs(ident) => {
                SeeDocs("function".to_owned(), format!("TODO/{}", ident)).fmt(f)
            }
            SeeErrorDocs => SeeDocs("error handling".to_owned(), "".to_owned()).fmt(f),
            SeeLangDocs => SeeDocs("language".to_owned(), "".to_owned()).fmt(f),
            SeeCodeDocs(code) => write!(f, "learn more at: https://errors.vrl.dev/{}", code),
            SeeDocs(kind, path) => {
                write!(
                    f,
                    "see {} documentation at: https://vector.dev/docs/reference/vrl/{}",
                    kind, path
                )
            }
        }
    }
}
