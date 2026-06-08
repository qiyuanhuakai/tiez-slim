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
        if rest.contains('|') {
            let options: Vec<String> = rest.split('|').map(|s| s.trim().to_string()).collect();
            if options.is_empty() || options.iter().any(|o| o.is_empty()) {
                return Err(InterpError::InvalidSyntax("empty picker option".into()));
            }
            Ok(InterpSegment { name, default: None, options: Some(options) })
        } else {
            Ok(InterpSegment { name, default: Some(rest.trim().to_string()), options: None })
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
