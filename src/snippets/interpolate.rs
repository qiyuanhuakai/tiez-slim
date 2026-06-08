//! Variable interpolation engine for snippet templates.
//!
//! Supports `{{var}}`, `{{var:default}}`, and `{{var:Select:a|b|c}}` syntax.

use std::collections::HashMap;

use chrono::Local;
use uuid::Uuid;

const MAX_CLIPBOARD_LEN: usize = 200;

#[derive(Debug, Clone, PartialEq)]
pub enum InterpError {
    MissingVariable(String),
    InvalidSyntax(String),
}

impl std::fmt::Display for InterpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterpError::MissingVariable(name) => write!(f, "missing variable: {name}"),
            InterpError::InvalidSyntax(msg) => write!(f, "invalid syntax: {msg}"),
        }
    }
}

impl std::error::Error for InterpError {}

#[derive(Debug, Clone, PartialEq)]
pub struct InterpSegment {
    pub name: String,
    pub default: Option<String>,
    pub options: Option<Vec<String>>,
}

pub fn parse_template(template: &str) -> Result<Vec<Token>, InterpError> {
    let mut tokens = Vec::new();
    let mut chars = template.char_indices().peekable();
    let mut literal_start = 0;

    while let Some((i, ch)) = chars.next() {
        if ch == '{'
            && let Some(&(_, '{')) = chars.peek()
        {
            chars.next();
            if literal_start < i {
                tokens.push(Token::Literal(template[literal_start..i].to_string()));
            }
            let var_start = i + 2;
            let mut depth = 1;
            let mut j = var_start;
            for (k, c) in template[var_start..].char_indices() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        j = var_start + k;
                        break;
                    }
                }
            }
            if depth != 0 {
                return Err(InterpError::InvalidSyntax("unmatched {{".into()));
            }
            if !template[j..].starts_with("}}") {
                return Err(InterpError::InvalidSyntax("expected }}".into()));
            }
            let inner = template[var_start..j].trim();
            if inner.is_empty() {
                return Err(InterpError::InvalidSyntax("empty variable name".into()));
            }
            tokens.push(Token::Variable(parse_variable(inner)?));
            literal_start = j + 2;
            continue;
        }
    }
    if literal_start < template.len() {
        tokens.push(Token::Literal(template[literal_start..].to_string()));
    }
    Ok(tokens)
}

fn parse_variable(inner: &str) -> Result<InterpSegment, InterpError> {
    if let Some((name, rest)) = inner.split_once(':') {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(InterpError::InvalidSyntax("empty variable name".into()));
        }
        let rest = rest.trim();
        if let Some(options_str) = rest.strip_prefix("Select:") {
            let options: Vec<String> = options_str
                .split('|')
                .map(|s| s.trim().to_string())
                .collect();
            if options.is_empty() || options.iter().any(|o| o.is_empty()) {
                return Err(InterpError::InvalidSyntax("empty picker option".into()));
            }
            Ok(InterpSegment {
                name,
                default: None,
                options: Some(options),
            })
        } else if rest.contains('|') {
            let options: Vec<String> = rest.split('|').map(|s| s.trim().to_string()).collect();
            if options.is_empty() || options.iter().any(|o| o.is_empty()) {
                return Err(InterpError::InvalidSyntax("empty picker option".into()));
            }
            Ok(InterpSegment {
                name,
                default: None,
                options: Some(options),
            })
        } else {
            Ok(InterpSegment {
                name,
                default: Some(rest.to_string()),
                options: None,
            })
        }
    } else {
        let name = inner.trim().to_string();
        if name.is_empty() {
            return Err(InterpError::InvalidSyntax("empty variable name".into()));
        }
        Ok(InterpSegment {
            name,
            default: None,
            options: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Literal(String),
    Variable(InterpSegment),
}

pub fn resolve_builtins(clipboard_content: Option<&str>) -> HashMap<String, String> {
    let now = Local::now();
    let mut map = HashMap::new();
    map.insert("date".into(), now.format("%Y-%m-%d").to_string());
    map.insert("time".into(), now.format("%H:%M:%S").to_string());
    map.insert("datetime".into(), now.format("%Y-%m-%d %H:%M:%S").to_string());
    map.insert("uuid".into(), Uuid::new_v4().to_string());
    if let Some(clip) = clipboard_content {
        let truncated: String = clip.chars().take(MAX_CLIPBOARD_LEN).collect();
        map.insert("clipboard".into(), truncated);
    }
    map
}

pub fn interpolate(
    template: &str,
    values: &HashMap<String, String>,
) -> Result<String, InterpError> {
    let tokens = parse_template(template)?;
    let mut result = String::new();
    for token in &tokens {
        match token {
            Token::Literal(s) => result.push_str(s),
            Token::Variable(seg) => {
                if let Some(val) = values.get(&seg.name) {
                    result.push_str(val);
                } else if let Some(ref default) = seg.default {
                    result.push_str(default);
                } else {
                    return Err(InterpError::MissingVariable(seg.name.clone()));
                }
            }
        }
    }
    Ok(result)
}

pub fn extract_variables(template: &str) -> Vec<InterpSegment> {
    let Ok(tokens) = parse_template(template) else {
        return Vec::new();
    };
    tokens
        .into_iter()
        .filter_map(|t| match t {
            Token::Variable(seg) => Some(seg),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_variable() {
        let mut vals = HashMap::new();
        vals.insert("name".into(), "Alice".into());
        assert_eq!(
            interpolate("Hello {{name}}!", &vals).unwrap(),
            "Hello Alice!"
        );
    }

    #[test]
    fn test_multiple_variables() {
        let mut vals = HashMap::new();
        vals.insert("org".into(), "rust-lang".into());
        vals.insert("repo".into(), "rust".into());
        vals.insert("num".into(), "1234".into());
        assert_eq!(
            interpolate("https://github.com/{{org}}/{{repo}}/pull/{{num}}", &vals).unwrap(),
            "https://github.com/rust-lang/rust/pull/1234"
        );
    }

    #[test]
    fn test_default_value() {
        let vals = HashMap::new();
        assert_eq!(
            interpolate("Hello {{name:World}}!", &vals).unwrap(),
            "Hello World!"
        );
    }

    #[test]
    fn test_default_overridden() {
        let mut vals = HashMap::new();
        vals.insert("name".into(), "Alice".into());
        assert_eq!(
            interpolate("Hello {{name:World}}!", &vals).unwrap(),
            "Hello Alice!"
        );
    }

    #[test]
    fn test_whitespace_tolerant() {
        let mut vals = HashMap::new();
        vals.insert("name".into(), "Bob".into());
        assert_eq!(interpolate("Hi {{ name }}!", &vals).unwrap(), "Hi Bob!");
    }

    #[test]
    fn test_picker_syntax_parsed() {
        let tokens = parse_template("Pick {{color:Select:red|green|blue}}").unwrap();
        assert_eq!(tokens.len(), 2);
        match &tokens[1] {
            Token::Variable(seg) => {
                assert_eq!(seg.name, "color");
                assert_eq!(seg.options.as_ref().unwrap(), &vec!["red", "green", "blue"]);
            }
            _ => panic!("expected variable token"),
        }
    }

    #[test]
    fn test_missing_variable_error() {
        let vals = HashMap::new();
        assert!(interpolate("{{missing}}", &vals).is_err());
    }

    #[test]
    fn test_no_variables() {
        let vals = HashMap::new();
        assert_eq!(interpolate("plain text", &vals).unwrap(), "plain text");
    }

    #[test]
    fn test_extract_variables() {
        let vars = extract_variables("{{a}} and {{b:default}} and {{c:X|Y|Z}}");
        assert_eq!(vars.len(), 3);
        assert_eq!(vars[0].name, "a");
        assert_eq!(vars[1].name, "b");
        assert_eq!(vars[1].default.as_deref(), Some("default"));
        assert_eq!(vars[2].name, "c");
        assert_eq!(vars[2].options.as_ref().unwrap(), &vec!["X", "Y", "Z"]);
    }

    #[test]
    fn builtin_date_is_yyyy_mm_dd() {
        let builtins = resolve_builtins(None);
        let date = builtins.get("date").expect("date builtin missing");
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");
    }

    #[test]
    fn builtin_time_is_hh_mm_ss() {
        let builtins = resolve_builtins(None);
        let time = builtins.get("time").expect("time builtin missing");
        assert_eq!(time.len(), 8);
        assert_eq!(&time[2..3], ":");
        assert_eq!(&time[5..6], ":");
    }

    #[test]
    fn builtin_datetime_format() {
        let builtins = resolve_builtins(None);
        let dt = builtins.get("datetime").expect("datetime builtin missing");
        assert_eq!(dt.len(), 19);
        assert_eq!(&dt[10..11], " ");
    }

    #[test]
    fn builtin_uuid_format() {
        let builtins = resolve_builtins(None);
        let uuid = builtins.get("uuid").expect("uuid builtin missing");
        assert_eq!(uuid.len(), 36);
        assert_eq!(uuid.chars().filter(|c| *c == '-').count(), 4);
    }

    #[test]
    fn builtin_clipboard_truncated() {
        let long_text = "a".repeat(500);
        let builtins = resolve_builtins(Some(&long_text));
        let clip = builtins.get("clipboard").expect("clipboard builtin missing");
        assert_eq!(clip.len(), 200);
    }

    #[test]
    fn builtin_clipboard_short_not_truncated() {
        let builtins = resolve_builtins(Some("hello"));
        let clip = builtins.get("clipboard").expect("clipboard builtin missing");
        assert_eq!(clip, "hello");
    }

    #[test]
    fn builtin_clipboard_none_not_present() {
        let builtins = resolve_builtins(None);
        assert!(!builtins.contains_key("clipboard"));
    }

    #[test]
    fn interpolate_with_builtins() {
        let builtins = resolve_builtins(None);
        let result = interpolate("Today is {{date}}", &builtins).unwrap();
        assert!(result.starts_with("Today is "));
        assert_eq!(result.len(), "Today is ".len() + 10);
    }
}
