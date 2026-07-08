//! Key-combo string splitting, shared by every platform backend so that
//! "ctrl++", "ctrl+", and "shift+%" mean the same thing on Linux, macOS,
//! and Windows. Name→keycode mapping stays per-platform; only the parse
//! is shared.

/// Split a combo on '+', treating a doubled '+' as the literal plus key
/// ("ctrl++" is ctrl and '+'). A dangling separator ("ctrl+") is an error:
/// silently pressing bare ctrl for a typo'd combo injects a wrong keystroke
/// and reports success.
pub fn split_combo(combo: &str) -> Result<Vec<String>, String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut chars = combo.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '+' {
            if cur.is_empty() {
                cur.push('+');
            }
            // A separator must be followed by another part.
            if chars.peek().is_none() && cur != "+" {
                return Err(format!("Malformed key combo '{}': trailing '+'", combo));
            }
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    if parts.is_empty() {
        return Err(format!("Malformed key combo '{}'", combo));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::split_combo;

    #[test]
    fn splits_simple_combo() {
        assert_eq!(split_combo("ctrl+shift+a").unwrap(), vec!["ctrl", "shift", "a"]);
    }
    #[test]
    fn doubled_plus_is_literal_plus() {
        assert_eq!(split_combo("ctrl++").unwrap(), vec!["ctrl", "+"]);
    }
    #[test]
    fn lone_plus_is_literal_plus() {
        assert_eq!(split_combo("+").unwrap(), vec!["+"]);
    }
    #[test]
    fn trailing_separator_errors() {
        assert!(split_combo("ctrl+").is_err());
    }
    #[test]
    fn empty_errors() {
        assert!(split_combo("").is_err());
    }
}
