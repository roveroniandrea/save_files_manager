//! Minimal parser for Valve's text KeyValues format (`.vdf`, `.acf`).
//!
//! Supports quoted and unquoted tokens, `\\` `\"` `\n` `\t` escapes, `//`
//! comments and `[$WIN32]`-style conditionals (ignored). Keys are matched
//! case-insensitively, as Steam does.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Obj(Obj),
}

/// An object: ordered `(key, value)` pairs (duplicate keys are kept).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Obj(pub Vec<(String, Value)>);

impl Obj {
    /// First value for `key` (case-insensitive).
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Str(s) => Some(s),
            Value::Obj(_) => None,
        }
    }

    pub fn obj(&self, key: &str) -> Option<&Obj> {
        match self.get(key)? {
            Value::Obj(o) => Some(o),
            Value::Str(_) => None,
        }
    }

    /// Follows a chain of nested object keys.
    pub fn path(&self, keys: &[&str]) -> Option<&Obj> {
        keys.iter().try_fold(self, |o, k| o.obj(k))
    }

    /// Child objects with their keys.
    pub fn objects(&self) -> impl Iterator<Item = (&str, &Obj)> {
        self.0.iter().filter_map(|(k, v)| match v {
            Value::Obj(o) => Some((k.as_str(), o)),
            Value::Str(_) => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VDF line {}: {}", self.line, self.message)
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Str(String),
    Open,
    Close,
}

struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    line: usize,
}

impl Lexer<'_> {
    fn err(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            line: self.line,
            message: message.into(),
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, ParseError> {
        loop {
            let Some(&c) = self.chars.peek() else {
                return Ok(None);
            };
            match c {
                '\n' => {
                    self.line += 1;
                    self.chars.next();
                }
                c if c.is_whitespace() || c == '\u{feff}' => {
                    self.chars.next();
                }
                '/' => {
                    self.chars.next();
                    if self.chars.peek() == Some(&'/') {
                        while self.chars.peek().is_some_and(|&c| c != '\n') {
                            self.chars.next();
                        }
                    } else {
                        return Ok(Some(Token::Str(self.unquoted("/".into()))));
                    }
                }
                '[' => {
                    // Conditional such as [$WIN32]: ignored.
                    for c in self.chars.by_ref() {
                        if c == ']' {
                            break;
                        }
                    }
                }
                '{' => {
                    self.chars.next();
                    return Ok(Some(Token::Open));
                }
                '}' => {
                    self.chars.next();
                    return Ok(Some(Token::Close));
                }
                '"' => {
                    self.chars.next();
                    return self.quoted().map(|s| Some(Token::Str(s)));
                }
                _ => return Ok(Some(Token::Str(self.unquoted(String::new())))),
            }
        }
    }

    fn quoted(&mut self) -> Result<String, ParseError> {
        let mut s = String::new();
        loop {
            match self.chars.next() {
                None => return Err(self.err("unterminated string")),
                Some('"') => return Ok(s),
                Some('\\') => match self.chars.next() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some(c @ ('\\' | '"')) => s.push(c),
                    Some(c) => {
                        s.push('\\');
                        s.push(c);
                    }
                    None => return Err(self.err("unterminated string")),
                },
                Some(c) => {
                    if c == '\n' {
                        self.line += 1;
                    }
                    s.push(c);
                }
            }
        }
    }

    fn unquoted(&mut self, mut s: String) -> String {
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() || matches!(c, '{' | '}' | '"') {
                break;
            }
            s.push(c);
            self.chars.next();
        }
        s
    }
}

/// Parses a whole document into its top-level object.
pub fn parse(text: &str) -> Result<Obj, ParseError> {
    let mut lexer = Lexer {
        chars: text.chars().peekable(),
        line: 1,
    };
    let obj = parse_obj(&mut lexer, false)?;
    Ok(obj)
}

fn parse_obj(lexer: &mut Lexer, nested: bool) -> Result<Obj, ParseError> {
    let mut obj = Obj::default();
    loop {
        let key = match lexer.next_token()? {
            None if nested => return Err(lexer.err("missing '}'")),
            None => return Ok(obj),
            Some(Token::Close) if nested => return Ok(obj),
            Some(Token::Close) => return Err(lexer.err("unexpected '}'")),
            Some(Token::Open) => return Err(lexer.err("unexpected '{'")),
            Some(Token::Str(key)) => key,
        };
        let value = match lexer.next_token()? {
            Some(Token::Str(s)) => Value::Str(s),
            Some(Token::Open) => Value::Obj(parse_obj(lexer, true)?),
            _ => return Err(lexer.err(format!("missing value for '{key}'"))),
        };
        obj.0.push((key, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_library_folders() {
        let text = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps"
		{
			"228980"		"58639359"
		}
	}
	"1" { "path" "D:\\SteamLibrary" }
}
"#;
        let doc = parse(text).unwrap();
        let folders = doc.obj("LibraryFolders").unwrap();
        let paths: Vec<_> = folders.objects().map(|(_, o)| o.str("path").unwrap()).collect();
        assert_eq!(paths, [r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]);
        assert_eq!(folders.path(&["0", "apps"]).unwrap().str("228980"), Some("58639359"));
    }

    #[test]
    fn handles_comments_conditionals_escapes_and_unquoted() {
        let text = "// header\nroot {\n key value [$WIN32]\n \"q\" \"a \\\"b\\\" \\\\c\\nd\"\n \"url\" \"http://x\" // trailing\n}";
        let doc = parse(text).unwrap();
        let root = doc.obj("root").unwrap();
        assert_eq!(root.str("KEY"), Some("value"));
        assert_eq!(root.str("q"), Some("a \"b\" \\c\nd"));
        assert_eq!(root.str("url"), Some("http://x"));
    }

    #[test]
    fn reports_errors_with_line() {
        assert_eq!(parse("a {\n b c\n").unwrap_err().line, 3);
        assert!(parse("a \"unterminated").is_err());
        assert!(parse("a").is_err());
        assert!(parse("}").is_err());
        assert_eq!(parse("").unwrap(), Obj::default());
    }
}
