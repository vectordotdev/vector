use std::fmt;

const VRL_DOCS_ROOT_URL: &str = "https://vrl.dev";
const VRL_ERROR_DOCS_ROOT_URL: &str = "https://errors.vrl.dev";
const VRL_FUNCS_ROOT_URL: &str = "https://functions.vrl.dev";

#[derive(Debug, PartialEq, Clone)]
pub enum Note {
    Hint(String, String),
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
            Hint(hint, url) => {
                if url.is_empty() {
                    write!(f, "hint: {}", hint)
                } else {
                    write!(f, "hint: {}\n    see: {}", hint, url)
                }
            },
            CoerceValue => {
                let coerce_funcs_url = format!("{}/#coerce-functions", VRL_FUNCS_ROOT_URL);

                Hint("coerce the value to the required type using a coercion function".to_owned(), coerce_funcs_url).fmt(f)
            }
            SeeFunctionDocs(ident) => {
                let func_url = format!("{}/{}", VRL_FUNCS_ROOT_URL, ident);
                SeeDocs("function".to_owned(), func_url).fmt(f)
            }
            SeeErrorDocs => {
                let error_handling_url = format!("{}/#handling", VRL_ERROR_DOCS_ROOT_URL);
                SeeDocs("error handling".to_owned(), error_handling_url).fmt(f)
            },
            SeeLangDocs => {
                let vrl_url = VRL_DOCS_ROOT_URL.to_owned();
                SeeDocs("language".to_owned(), vrl_url).fmt(f)
            },
            SeeCodeDocs(code) => {
                let error_code_url = format!("{}/{}", VRL_ERROR_DOCS_ROOT_URL, code);

                write!(f, "learn more about error code {} at {}", code, error_code_url)
            },
            SeeDocs(kind, url) => {
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
