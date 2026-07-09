use std::fmt;

/// A line/column position in the source XAML text (1-based, like compilers report).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextPos {
    pub line: u32,
    pub col: u32,
}

impl TextPos {
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

impl fmt::Display for TextPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum XamlError {
    #[error("XML error: {0}")]
    Xml(String),

    #[error("{pos}: {message}")]
    Parse { pos: TextPos, message: String },

    #[error("invalid markup extension `{input}`: {message}")]
    MarkupExtension { input: String, message: String },

    #[error("cannot convert `{input}` to {target}: {message}")]
    Convert {
        input: String,
        target: &'static str,
        message: String,
    },
}

impl XamlError {
    pub fn parse(pos: TextPos, message: impl Into<String>) -> Self {
        Self::Parse {
            pos,
            message: message.into(),
        }
    }

    pub fn convert(
        input: impl Into<String>,
        target: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self::Convert {
            input: input.into(),
            target,
            message: message.into(),
        }
    }
}

impl From<roxmltree::Error> for XamlError {
    fn from(e: roxmltree::Error) -> Self {
        Self::Xml(e.to_string())
    }
}

pub type XamlResult<T> = Result<T, XamlError>;
