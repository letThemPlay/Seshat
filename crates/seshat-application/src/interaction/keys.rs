use std::fmt;

use thiserror::Error;

/// A key, independent of the UI toolkit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// A printable character, already shifted (`G`, `:`, `[`). Space is
    /// `Char(' ')`.
    Char(char),
    Enter,
    Escape,
    Backspace,
    Tab,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
}

/// A key with the modifiers that matter for bindings. Shift is folded into
/// characters (`G`, not Shift+g).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyPress {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
}

impl KeyPress {
    pub fn plain(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            alt: false,
        }
    }

    pub fn char(c: char) -> Self {
        Self::plain(Key::Char(c))
    }

    pub fn ctrl(c: char) -> Self {
        Self {
            key: Key::Char(c),
            ctrl: true,
            alt: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum KeyParseError {
    #[error("empty key sequence")]
    Empty,
    #[error("unclosed “<” in “{0}”")]
    Unclosed(String),
    #[error("unknown key “<{0}>”")]
    UnknownKey(String),
}

const NAMED: &[(&str, Key)] = &[
    ("CR", Key::Enter),
    ("Enter", Key::Enter),
    ("Return", Key::Enter),
    ("Esc", Key::Escape),
    ("BS", Key::Backspace),
    ("Tab", Key::Tab),
    ("Del", Key::Delete),
    ("Space", Key::Char(' ')),
    ("lt", Key::Char('<')),
    ("Up", Key::Up),
    ("Down", Key::Down),
    ("Left", Key::Left),
    ("Right", Key::Right),
    ("Home", Key::Home),
    ("End", Key::End),
    ("PageUp", Key::PageUp),
    ("PageDown", Key::PageDown),
];

/// Parses Vim-style key notation: `gg`, `zz`, `<C-d>`, `<CR>`, `<A-x>`,
/// `<Space>`, `<lt>` for a literal `<`.
pub fn parse_sequence(text: &str) -> Result<Vec<KeyPress>, KeyParseError> {
    let mut keys = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '<' {
            keys.push(KeyPress::char(c));
            continue;
        }
        let mut inner = String::new();
        loop {
            match chars.next() {
                Some('>') => break,
                Some(c) => inner.push(c),
                None => return Err(KeyParseError::Unclosed(text.to_owned())),
            }
        }
        keys.push(parse_named(&inner)?);
    }
    if keys.is_empty() {
        return Err(KeyParseError::Empty);
    }
    Ok(keys)
}

fn parse_named(inner: &str) -> Result<KeyPress, KeyParseError> {
    let mut press = KeyPress::plain(Key::Escape);
    let mut rest = inner;
    loop {
        if let Some(r) = rest.strip_prefix("C-") {
            press.ctrl = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("A-") {
            press.alt = true;
            rest = r;
        } else {
            break;
        }
    }
    let mut single = rest.chars();
    press.key = match (single.next(), single.next()) {
        (Some(c), None) if press.ctrl || press.alt => Key::Char(c),
        _ => NAMED
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(rest))
            .map(|(_, key)| *key)
            .ok_or_else(|| KeyParseError::UnknownKey(inner.to_owned()))?,
    };
    Ok(press)
}

impl fmt::Display for KeyPress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.key {
            Key::Char('<') => Some("lt"),
            Key::Char(' ') => Some("Space"),
            Key::Char(_) => None,
            other => NAMED.iter().find(|(_, k)| *k == other).map(|(n, _)| *n),
        };
        let modified = self.ctrl || self.alt;
        if !modified && let (Key::Char(c), None) = (self.key, name) {
            return write!(f, "{c}");
        }
        f.write_str("<")?;
        if self.ctrl {
            f.write_str("C-")?;
        }
        if self.alt {
            f.write_str("A-")?;
        }
        match (self.key, name) {
            (Key::Char(c), None) => write!(f, "{c}>"),
            (_, Some(n)) => write!(f, "{n}>"),
            (_, None) => f.write_str("?>"),
        }
    }
}

/// Formats a sequence back into notation (for help text and messages).
pub fn format_sequence(keys: &[KeyPress]) -> String {
    keys.iter().map(ToString::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_named_and_modified_keys() {
        assert_eq!(
            parse_sequence("gg").unwrap(),
            [KeyPress::char('g'), KeyPress::char('g')]
        );
        assert_eq!(parse_sequence("<C-d>").unwrap(), [KeyPress::ctrl('d')]);
        assert_eq!(
            parse_sequence("<CR>").unwrap(),
            [KeyPress::plain(Key::Enter)]
        );
        assert_eq!(
            parse_sequence("<A-C-x>").unwrap(),
            [KeyPress {
                key: Key::Char('x'),
                ctrl: true,
                alt: true
            }]
        );
        assert_eq!(parse_sequence("<lt>").unwrap(), [KeyPress::char('<')]);
        assert_eq!(parse_sequence("[").unwrap(), [KeyPress::char('[')]);
    }

    #[test]
    fn rejects_bad_notation() {
        assert_eq!(parse_sequence(""), Err(KeyParseError::Empty));
        assert!(matches!(
            parse_sequence("<C-d"),
            Err(KeyParseError::Unclosed(_))
        ));
        assert!(matches!(
            parse_sequence("<Bogus>"),
            Err(KeyParseError::UnknownKey(_))
        ));
    }

    #[test]
    fn formatting_round_trips() {
        for text in [
            "gg", "<C-d>", "<CR>", "<BS>", "zz", "<Esc>", "<Space>", "<lt>", "<A-x>",
        ] {
            assert_eq!(format_sequence(&parse_sequence(text).unwrap()), text);
        }
    }
}
