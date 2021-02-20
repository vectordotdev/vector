use std::fmt;

const VRL_DOCS_ROOT_URL: &str = "https://vrl.dev";

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
    #[doc(hidden)]
    Basic(String),
}

impl Note {
    pub fn solution(title: impl Into<String>, content: Vec<impl Into<String>>) -> Vec<Self> {
        let mut notes = vec![Self::Basic(format!("try: {}", title.into()))];

        notes.push(Self::Basic(" ".to_owned()));
        for line in content {
            notes.push(Self::Basic(format!("    {}", line.into())));
        }
        notes.push(Self::Basic(" ".to_owned()));
        notes
    }
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
                let url = if path.is_empty() {
                    VRL_DOCS_ROOT_URL.into()
                } else {
                    format!("{}/{}", VRL_DOCS_ROOT_URL, path)
                };

                write!(
                    f,
                    "see {} documentation at {}",
                    kind, url
                )
            }
            Basic(string) => write!(f, "{}", string),
        }
    }
}
