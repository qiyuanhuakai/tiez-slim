//! Variable interpolation engine for snippet templates.
//!
//! Supports `{{var}}`, `{{var:default}}`, and `{{var:Select:a|b|c}}` syntax.

use std::collections::HashMap;

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
        if ch == '{' {
            if let Some(&(_, '{')) = chars.peek() {
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
            let options: Vec<String> = options_str.split('|').map(|s| s.trim().to_string()).collect();
            if options.is_empty() || options.iter().any(|o| o.is_empty()) {
                return Err(InterpError::InvalidSyntax("empty picker option".into()));
            }
            Ok(InterpSegment { name, default: None, options: Some(options) })
        } else if rest.contains('|') {
            let options: Vec<String> = rest.split('|').map(|s| s.trim().to_string()).collect();
            if options.is_empty() || options.iter().any(|o| o.is_empty()) {
                return Err(InterpError::InvalidSyntax("empty picker option".into()));
            }
            Ok(InterpSegment { name, default: None, options: Some(options) })
        } else {
            Ok(InterpSegment { name, default: Some(rest.to_string()), options: None })
        }
    } else {
        let name = inner.trim().to_string();
        if name.is_empty() {
            return Err(InterpError::InvalidSyntax("empty variable name".into()));
        }
        Ok(InterpSegment { name, default: None, options: None })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Literal(String),
    Variable(InterpSegment),
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
        assert_eq!(interpolate("Hello {{name}}!", &vals).unwrap(), "Hello Alice!");
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
}
