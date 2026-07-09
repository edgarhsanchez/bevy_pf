//! Parser for XAML markup-extension syntax, conformant with WPF's actual
//! scanner (`MeScanner.cs` / `MePullParser.cs` in dotnet/wpf — see
//! `docs/wpf-conformance-notes.md` §3 for the audited rules).
//!
//! ```text
//! extension   := '{' WS* name (WS args)? WS* '}'
//! args        := arg (WS* ',' WS* arg)*
//! arg         := named | positional          (positional may not follow named)
//! named       := key WS* '=' WS* value
//! value       := extension | quoted | bareword
//! quoted      := ('\'' | '"') (escaped-char | any-but-open-quote)* same-quote
//! bareword    := chars up to ',' '}' '=' at brace depth 0; literal balanced
//!                braces allowed, but ',' '=' inside them are errors; quotes
//!                mid-bareword are errors; trailing WS trimmed
//! ```
//!
//! - Whitespace is the ASCII set `{space, \t, \n, \r, \x0C}` only.
//! - An attribute value beginning with `{}` is an escape: the remainder is a
//!   literal string. `{` followed by whitespace *is* an extension (WPF parses
//!   `"{ Binding }"`); a lone `{` is a parse error.
//! - A quoted value that itself starts with `{` (and not `{}`) is re-parsed
//!   as a nested extension, per `MePullParser`.
//! - Anything after the closing `}` — including whitespace — is an error.

use crate::error::{XamlError, XamlResult};

/// A parsed markup extension such as `{Binding Path=Name, Mode=TwoWay}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkupExtension {
    /// Extension name with any namespace prefix kept, e.g. `Binding`,
    /// `StaticResource`, `x:Static`, `x:Null`.
    pub name: String,
    /// Positional (unnamed) arguments in order.
    pub positional: Vec<MarkupValue>,
    /// Named arguments in order of appearance.
    pub named: Vec<(String, MarkupValue)>,
}

impl MarkupExtension {
    pub fn arg(&self, name: &str) -> Option<&MarkupValue> {
        self.named
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }

    /// The first positional argument as a string, if any.
    pub fn first_positional_str(&self) -> Option<&str> {
        self.positional.first().and_then(MarkupValue::as_str)
    }
}

/// A value inside a markup extension: a string or a nested extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkupValue {
    Str(String),
    Extension(Box<MarkupExtension>),
}

impl MarkupValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            MarkupValue::Str(s) => Some(s),
            MarkupValue::Extension(_) => None,
        }
    }
}

/// The ASCII whitespace set WPF's markup-extension scanner uses
/// (`KnownStrings.WhitespaceChars`).
fn is_me_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

/// Does this attribute text denote a markup extension (rather than a literal)?
///
/// WPF rules (`XamlText.LooksLikeAMarkupExtension`): it must start with `{`
/// and not be the `{}` literal escape. `{` followed by whitespace *is* an
/// extension; a malformed one is then a parse error, exactly like WPF.
pub fn is_extension(text: &str) -> bool {
    let mut chars = text.chars();
    if chars.next() != Some('{') {
        return false;
    }
    !matches!(chars.next(), Some('}'))
}

/// Strip the `{}` literal-escape prefix if present.
pub fn unescape_literal(text: &str) -> &str {
    text.strip_prefix("{}").unwrap_or(text)
}

/// Parse a complete markup-extension attribute value like
/// `{Binding Path=Name, Converter={StaticResource conv}}`.
///
/// The input must start at `{` and end exactly at the matching `}` — WPF
/// errors on any trailing characters, including whitespace.
pub fn parse_extension(input: &str) -> XamlResult<MarkupExtension> {
    let mut p = Parser {
        input,
        chars: input.char_indices().peekable(),
    };
    let ext = p.parse_extension()?;
    if let Some(&(i, c)) = p.chars.peek() {
        return Err(err(
            input,
            format!("unexpected trailing character `{c}` at byte {i} after `}}`"),
        ));
    }
    Ok(ext)
}

fn err(input: &str, message: impl Into<String>) -> XamlError {
    XamlError::MarkupExtension {
        input: input.to_string(),
        message: message.into(),
    }
}

/// One scanned token: text, whether it was quoted, or a nested extension.
struct Token {
    text: String,
    quoted: bool,
    nested: Option<MarkupExtension>,
}

struct Parser<'a> {
    input: &'a str,
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while let Some(&(_, c)) = self.chars.peek() {
            if is_me_whitespace(c) {
                self.chars.next();
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, expected: char) -> XamlResult<()> {
        match self.chars.next() {
            Some((_, c)) if c == expected => Ok(()),
            Some((i, c)) => Err(err(
                self.input,
                format!("expected `{expected}` but found `{c}` at byte {i}"),
            )),
            None => Err(err(
                self.input,
                format!("expected `{expected}` but reached end of input"),
            )),
        }
    }

    fn parse_extension(&mut self) -> XamlResult<MarkupExtension> {
        self.expect('{')?;
        self.skip_ws();

        // Extension name: terminates at whitespace, ',', '=', '{', or '}'.
        let mut name = String::new();
        while let Some(&(_, c)) = self.chars.peek() {
            if is_me_whitespace(c) || matches!(c, '}' | ',' | '=' | '{') {
                break;
            }
            name.push(c);
            self.chars.next();
        }
        if name.is_empty() {
            return Err(err(self.input, "missing extension name"));
        }

        let mut ext = MarkupExtension {
            name,
            positional: Vec::new(),
            named: Vec::new(),
        };

        loop {
            self.skip_ws();
            match self.chars.peek() {
                Some(&(_, '}')) => {
                    self.chars.next();
                    return Ok(ext);
                }
                Some(&(i, '=')) => {
                    return Err(err(
                        self.input,
                        format!("expected an argument name before `=` at byte {i}"),
                    ));
                }
                Some(&(i, ',')) => {
                    return Err(err(
                        self.input,
                        format!("expected an argument before `,` at byte {i}"),
                    ));
                }
                None => return Err(err(self.input, "unterminated extension (missing `}`)")),
                Some(_) => {}
            }

            let token = self.parse_value_token()?;
            self.skip_ws();

            match self.chars.peek() {
                // `key = value`
                Some(&(_, '=')) => {
                    if token.nested.is_some() {
                        return Err(err(self.input, "extension cannot be an argument name"));
                    }
                    if token.quoted {
                        return Err(err(self.input, "quoted string cannot be an argument name"));
                    }
                    if token.text.is_empty() {
                        return Err(err(self.input, "empty argument name"));
                    }
                    self.chars.next(); // consume '='
                    self.skip_ws();
                    let value = self.parse_value_token()?;
                    if value.nested.is_none() && !value.quoted && value.text.is_empty() {
                        return Err(err(
                            self.input,
                            format!("missing value for argument `{}`", token.text),
                        ));
                    }
                    let v = match value.nested {
                        Some(n) => MarkupValue::Extension(Box::new(n)),
                        None => MarkupValue::Str(value.text),
                    };
                    ext.named.push((token.text, v));
                }
                _ => {
                    // Positional argument. WPF rejects positionals after any
                    // named argument (`MePullParser` states 296-330).
                    if !ext.named.is_empty() {
                        return Err(err(
                            self.input,
                            "positional argument after named argument",
                        ));
                    }
                    if token.nested.is_none() && !token.quoted && token.text.is_empty() {
                        return Err(err(self.input, "empty argument"));
                    }
                    let v = match token.nested {
                        Some(n) => MarkupValue::Extension(Box::new(n)),
                        None => MarkupValue::Str(token.text),
                    };
                    ext.positional.push(v);
                }
            }

            self.skip_ws();
            match self.chars.peek() {
                Some(&(_, ',')) => {
                    self.chars.next();
                    self.skip_ws();
                    if matches!(self.chars.peek(), Some(&(_, '}'))) {
                        return Err(err(self.input, "expected an argument after `,`"));
                    }
                }
                Some(&(_, '}')) => {} // handled at loop top
                Some(&(i, c)) => {
                    return Err(err(
                        self.input,
                        format!("expected `,` or `}}` but found `{c}` at byte {i}"),
                    ));
                }
                None => {}
            }
        }
    }

    /// Parse one token: a nested extension, a quoted string, or a bareword.
    fn parse_value_token(&mut self) -> XamlResult<Token> {
        match self.chars.peek() {
            Some(&(_, '{')) => {
                // `{}` at the start of a value is WPF's literal escape:
                // `StringFormat={}{0:00}` means the value is `{0:00}`. The
                // escaped remainder is literal text, so `,`/`=` inside its
                // balanced braces are content, not errors.
                let mut lookahead = self.chars.clone();
                lookahead.next();
                if matches!(lookahead.peek(), Some(&(_, '}'))) {
                    self.chars.next(); // '{'
                    self.chars.next(); // '}'
                    return self.parse_bareword_impl(true);
                }
                let ext = self.parse_extension()?;
                Ok(Token {
                    text: String::new(),
                    quoted: false,
                    nested: Some(ext),
                })
            }
            Some(&(_, quote @ ('\'' | '"'))) => {
                // Either quote char opens a string; only the same one closes
                // it (`MeScanner.cs:462-471`). Quoted values are not trimmed.
                self.chars.next(); // opening quote
                let mut s = String::new();
                loop {
                    match self.chars.next() {
                        Some((_, '\\')) => match self.chars.next() {
                            Some((_, c)) => s.push(c),
                            None => {
                                return Err(err(self.input, "dangling escape in quoted string"));
                            }
                        },
                        Some((_, c)) if c == quote => break,
                        Some((_, c)) => s.push(c),
                        None => return Err(err(self.input, "unterminated quoted string")),
                    }
                }
                // A quoted value that is itself an extension is re-parsed as
                // one (`MePullParser.cs:350-359`); `{}`-escaped stays literal.
                if s.starts_with('{') && !s.starts_with("{}") {
                    let nested = parse_extension(&s)?;
                    return Ok(Token {
                        text: String::new(),
                        quoted: true,
                        nested: Some(nested),
                    });
                }
                let text = s.strip_prefix("{}").unwrap_or(&s).to_string();
                Ok(Token {
                    text,
                    quoted: true,
                    nested: None,
                })
            }
            Some(_) => self.parse_bareword(),
            None => Err(err(self.input, "unexpected end of input in value")),
        }
    }

    fn parse_bareword(&mut self) -> XamlResult<Token> {
        self.parse_bareword_impl(false)
    }

    /// Bareword: read until ',', '}', or '=' at brace depth 0. Balanced
    /// literal braces are allowed for v3 compatibility; ',' and '=' inside
    /// them are errors unless `literal_escape` (a `{}`-escaped value, whose
    /// remainder is literal text). `[`...`]` and `(`...`)` group like WPF's
    /// bracket characters (`Binding` declares them for indexer and attached
    /// paths — `Path=Items[0,1]`, `Path=(Grid.Row)`): quotes, commas, and
    /// equals inside are literal. Backslash escapes the next character.
    /// Trailing whitespace is trimmed.
    fn parse_bareword_impl(&mut self, literal_escape: bool) -> XamlResult<Token> {
        let mut s = String::new();
        let mut depth = 0usize;
        let mut brackets: Vec<char> = Vec::new(); // stack of expected closers
        while let Some(&(i, c)) = self.chars.peek() {
            if let Some(&closer) = brackets.last() {
                // Inside [...] or (...): content is literal (quotes, commas,
                // equals), brackets nest, backslash still escapes.
                match c {
                    '\\' => {
                        self.chars.next();
                        match self.chars.next() {
                            Some((_, e)) => s.push(e),
                            None => {
                                return Err(err(self.input, "dangling escape in value"));
                            }
                        }
                        continue;
                    }
                    '[' => brackets.push(']'),
                    '(' => brackets.push(')'),
                    _ if c == closer => {
                        brackets.pop();
                    }
                    _ => {}
                }
                s.push(c);
                self.chars.next();
                continue;
            }
            match c {
                '\\' => {
                    self.chars.next();
                    match self.chars.next() {
                        Some((_, e)) => s.push(e),
                        None => {
                            return Err(err(self.input, "dangling escape in value"));
                        }
                    }
                }
                '[' => {
                    brackets.push(']');
                    s.push(c);
                    self.chars.next();
                }
                '(' => {
                    brackets.push(')');
                    s.push(c);
                    self.chars.next();
                }
                '\'' | '"' => {
                    return Err(err(
                        self.input,
                        format!("unexpected quote inside unquoted value at byte {i}"),
                    ));
                }
                '{' => {
                    depth += 1;
                    s.push(c);
                    self.chars.next();
                }
                '}' if depth > 0 => {
                    depth -= 1;
                    s.push(c);
                    self.chars.next();
                }
                ',' | '=' if depth > 0 => {
                    if literal_escape {
                        s.push(c);
                        self.chars.next();
                    } else {
                        return Err(err(
                            self.input,
                            format!("`{c}` is not allowed inside literal braces (byte {i})"),
                        ));
                    }
                }
                ',' | '}' | '=' => break,
                _ => {
                    s.push(c);
                    self.chars.next();
                }
            }
        }
        if !brackets.is_empty() {
            return Err(err(self.input, "unterminated bracket in value"));
        }
        while s.ends_with(is_me_whitespace) {
            s.pop();
        }
        Ok(Token {
            text: s,
            quoted: false,
            nested: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> MarkupValue {
        MarkupValue::Str(v.to_string())
    }

    #[test]
    fn detects_extensions() {
        assert!(is_extension("{Binding}"));
        assert!(is_extension("{StaticResource key}"));
        // WPF treats `{ ...` (whitespace after brace) as an extension and a
        // lone `{` as a (failing) extension, not literal text.
        assert!(is_extension("{ Binding }"));
        assert!(is_extension("{"));
        assert!(!is_extension("plain"));
        assert!(!is_extension("{} literal with braces"));
        assert!(!is_extension(""));
    }

    #[test]
    fn unescapes_literal_prefix() {
        assert_eq!(unescape_literal("{}{Binding}"), "{Binding}");
        assert_eq!(unescape_literal("hello"), "hello");
    }

    #[test]
    fn parses_bare_extension() {
        let e = parse_extension("{Binding}").unwrap();
        assert_eq!(e.name, "Binding");
        assert!(e.positional.is_empty());
        assert!(e.named.is_empty());
    }

    #[test]
    fn parses_positional() {
        let e = parse_extension("{StaticResource MyBrush}").unwrap();
        assert_eq!(e.name, "StaticResource");
        assert_eq!(e.positional, vec![s("MyBrush")]);
    }

    #[test]
    fn parses_binding_path_positional() {
        let e = parse_extension("{Binding Customer.Name}").unwrap();
        assert_eq!(e.positional, vec![s("Customer.Name")]);
    }

    #[test]
    fn parses_named_args() {
        let e = parse_extension("{Binding Path=Name, Mode=TwoWay}").unwrap();
        assert_eq!(e.name, "Binding");
        assert_eq!(
            e.named,
            vec![
                ("Path".to_string(), s("Name")),
                ("Mode".to_string(), s("TwoWay")),
            ]
        );
    }

    #[test]
    fn parses_positional_then_named() {
        let e = parse_extension("{Binding Name, Mode=OneWay}").unwrap();
        assert_eq!(e.positional, vec![s("Name")]);
        assert_eq!(e.named, vec![("Mode".to_string(), s("OneWay"))]);
    }

    #[test]
    fn rejects_positional_after_named() {
        // MePullParser rejects this; parenthesized DRT cases that appear to
        // mix orders are single tokens, not mixed arguments.
        assert!(parse_extension("{Binding Mode=OneWay, Name}").is_err());
    }

    #[test]
    fn parses_nested_extension() {
        let e =
            parse_extension("{Binding Path=Name, Converter={StaticResource conv}}").unwrap();
        let conv = e.arg("Converter").unwrap();
        match conv {
            MarkupValue::Extension(inner) => {
                assert_eq!(inner.name, "StaticResource");
                assert_eq!(inner.positional, vec![s("conv")]);
            }
            _ => panic!("expected nested extension"),
        }
    }

    #[test]
    fn parses_deeply_nested() {
        let e = parse_extension(
            "{Binding RelativeSource={RelativeSource Mode=FindAncestor, AncestorType={x:Type ListBox}}}",
        )
        .unwrap();
        let rs = e.arg("RelativeSource").unwrap();
        let MarkupValue::Extension(rs) = rs else {
            panic!("expected extension")
        };
        assert_eq!(rs.name, "RelativeSource");
        let at = rs.arg("AncestorType").unwrap();
        let MarkupValue::Extension(at) = at else {
            panic!("expected extension")
        };
        assert_eq!(at.name, "x:Type");
        assert_eq!(at.positional, vec![s("ListBox")]);
    }

    #[test]
    fn parses_quoted_values() {
        let e = parse_extension("{Binding Path=Name, StringFormat='Hello, {0}!'}").unwrap();
        assert_eq!(e.arg("StringFormat"), Some(&s("Hello, {0}!")));
    }

    #[test]
    fn parses_double_quoted_values() {
        // `"` is a quote char too; only the opening kind closes the string.
        let e = parse_extension(r#"{Binding StringFormat="Hi, {0}", Path=A}"#).unwrap();
        assert_eq!(e.arg("StringFormat"), Some(&s("Hi, {0}")));
        let e = parse_extension(r#"{Binding StringFormat="It's fine"}"#).unwrap();
        assert_eq!(e.arg("StringFormat"), Some(&s("It's fine")));
    }

    #[test]
    fn quoted_extension_reparses_as_nested() {
        let e = parse_extension("{Binding RelativeSource='{RelativeSource Self}'}").unwrap();
        let MarkupValue::Extension(rs) = e.arg("RelativeSource").unwrap() else {
            panic!("expected nested extension from quoted value")
        };
        assert_eq!(rs.name, "RelativeSource");
        assert_eq!(rs.positional, vec![s("Self")]);
    }

    #[test]
    fn quoted_literal_escape_stays_literal() {
        let e = parse_extension("{Binding StringFormat='{}{0}'}").unwrap();
        assert_eq!(e.arg("StringFormat"), Some(&s("{0}")));
    }

    #[test]
    fn parses_quoted_with_escapes() {
        let e = parse_extension(r"{Binding StringFormat='It\'s {0}'}").unwrap();
        assert_eq!(e.arg("StringFormat"), Some(&s("It's {0}")));
    }

    #[test]
    fn literal_escape_inside_value() {
        // WPF: `{}` at the start of a member value escapes the rest.
        let e = parse_extension("{Binding Minute, StringFormat={}{0:00}}").unwrap();
        assert_eq!(e.arg("StringFormat"), Some(&s("{0:00}")));

        let e = parse_extension(
            "{Binding Value, ElementName=progress1, FallbackValue=0%, StringFormat={}{0:F0}%}",
        )
        .unwrap();
        assert_eq!(e.arg("StringFormat"), Some(&s("{0:F0}%")));
        assert_eq!(e.arg("FallbackValue"), Some(&s("0%")));

        // The escaped remainder is literal: commas inside its braces are
        // content (real-world .NET format strings).
        let e = parse_extension("{Binding Damage, StringFormat={}{0:#,.##} K}").unwrap();
        assert_eq!(e.arg("StringFormat"), Some(&s("{0:#,.##} K")));
    }

    #[test]
    fn parens_group_like_brackets() {
        // Parenthesized attached-property paths and DRT bracket cases.
        let e = parse_extension("{Binding Path=(Grid.Row)}").unwrap();
        assert_eq!(e.arg("Path"), Some(&s("(Grid.Row)")));
        let e = parse_extension(r#"{local:BCMarkup Animals("poodle,doodle")}"#).unwrap();
        assert_eq!(
            e.positional,
            vec![s(r#"Animals("poodle,doodle")"#)]
        );
    }

    #[test]
    fn balanced_literal_braces_in_bareword() {
        let e = parse_extension("{Binding Path=a{b}c}").unwrap();
        assert_eq!(e.arg("Path"), Some(&s("a{b}c")));
    }

    #[test]
    fn comma_or_equals_inside_literal_braces_is_error() {
        assert!(parse_extension("{Binding Path=a{1,2}}").is_err());
        assert!(parse_extension("{Binding Path=a{x=y}}").is_err());
    }

    #[test]
    fn parses_x_null_and_x_static() {
        let e = parse_extension("{x:Null}").unwrap();
        assert_eq!(e.name, "x:Null");
        let e = parse_extension("{x:Static SystemColors.ControlBrushKey}").unwrap();
        assert_eq!(e.name, "x:Static");
        assert_eq!(e.positional, vec![s("SystemColors.ControlBrushKey")]);
    }

    #[test]
    fn parses_template_binding() {
        let e = parse_extension("{TemplateBinding Background}").unwrap();
        assert_eq!(e.name, "TemplateBinding");
        assert_eq!(e.positional, vec![s("Background")]);
    }

    #[test]
    fn bareword_preserves_internal_spaces() {
        let e = parse_extension("{DynamicResource My Key Name}").unwrap();
        assert_eq!(e.positional, vec![s("My Key Name")]);
    }

    #[test]
    fn escaped_comma_in_bareword() {
        let e = parse_extension(r"{Binding Path=Items[0\,1]}").unwrap();
        assert_eq!(e.arg("Path"), Some(&s("Items[0,1]")));
    }

    #[test]
    fn brackets_group_indexer_paths() {
        // WPF bracket characters (Binding declares [ ] for indexers):
        // `,`, `=`, and quotes are literal inside.
        let e = parse_extension("{Binding Path=Items[0,1]}").unwrap();
        assert_eq!(e.arg("Path"), Some(&s("Items[0,1]")));
        let e = parse_extension(r#"{Binding Path=Animals["poodle,doodle"]}"#).unwrap();
        assert_eq!(e.arg("Path"), Some(&s(r#"Animals["poodle,doodle"]"#)));
        assert!(parse_extension("{Binding Path=Items[0}").is_err());
    }

    #[test]
    fn whitespace_tolerance_inside_braces() {
        // `{ Binding` with a space after the brace is valid WPF.
        let e = parse_extension("{ Binding  Path = Name , Mode = TwoWay }").unwrap();
        assert_eq!(e.name, "Binding");
        assert_eq!(e.arg("Path"), Some(&s("Name")));
        assert_eq!(e.arg("Mode"), Some(&s("TwoWay")));
    }

    #[test]
    fn me_whitespace_set_is_ascii_only() {
        // NBSP is not scanner whitespace: it joins the extension name.
        let e = parse_extension("{Binding\u{A0}Title}").unwrap();
        assert_eq!(e.name, "Binding\u{A0}Title");
        assert!(e.positional.is_empty());
    }

    #[test]
    fn errors_on_unterminated() {
        assert!(parse_extension("{Binding Path=Name").is_err());
        assert!(parse_extension("{").is_err());
        assert!(parse_extension("{Binding StringFormat='oops}").is_err());
    }

    #[test]
    fn errors_on_trailing_garbage_and_whitespace() {
        assert!(parse_extension("{Binding} extra").is_err());
        // WPF errors even on trailing whitespace after the closing brace.
        assert!(parse_extension("{Binding} ").is_err());
    }

    #[test]
    fn errors_on_empty_arguments() {
        assert!(parse_extension("{Binding Path=}").is_err());
        assert!(parse_extension("{Binding ,Path=x}").is_err());
        assert!(parse_extension("{Binding Path=x,}").is_err());
        assert!(parse_extension("{Foo=Bar}").is_err());
    }

    #[test]
    fn errors_on_quote_mid_bareword() {
        assert!(parse_extension("{Binding Path=a'b}").is_err());
        assert!(parse_extension(r#"{Binding Path=a"b}"#).is_err());
    }

    #[test]
    fn relative_source_shorthand() {
        let e = parse_extension("{Binding RelativeSource={RelativeSource Self}, Path=Width}")
            .unwrap();
        let MarkupValue::Extension(rs) = e.arg("RelativeSource").unwrap() else {
            panic!()
        };
        assert_eq!(rs.positional, vec![s("Self")]);
    }
}
