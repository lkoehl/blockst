//! Text -> NEPO blocks.
//!
//! The surface syntax is indentation-based, as in the prototype brief:
//!
//! ```text
//! Start
//!   Zeige Text "Hallo"
//!   Wiederhole unendlich oft
//!     Schalte RGB LED an (#ff0000)
//!   Ende
//! ```
//!
//! A line is matched against the localised pattern of every block the selected
//! platform offers. `Ende` is optional — the indentation already says where a
//! mouth closes — but accepted, because teaching material tends to write it.

use crate::catalog::{self, Binding, Language, Pattern, Token};
use crate::model::{BlockSpec, DocumentSpec, ScriptSpec};
use crate::render::render_document;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Request {
    pub code: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_platform")]
    pub platform: String,
    pub theme: Option<String>,
    pub scale: Option<f32>,
    #[serde(default)]
    pub font: String,
    #[serde(default)]
    pub widths: Option<HashMap<String, f32>>,
}

fn default_language() -> String {
    "de".to_string()
}

fn default_platform() -> String {
    "calliope".to_string()
}

struct Line {
    indent: usize,
    text: String,
    number: usize,
}

fn scan(code: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for (index, raw) in code.lines().enumerate() {
        let without_comment = match raw.find("//") {
            Some(at) => &raw[..at],
            None => raw,
        };
        if without_comment.trim().is_empty() {
            continue;
        }
        // A tab counts as two columns, which is enough to keep relative depth
        // consistent for any file that does not mix tabs and spaces mid-line.
        let indent = without_comment
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .map(|ch| if ch == '\t' { 2 } else { 1 })
            .sum();
        lines.push(Line {
            indent,
            text: without_comment.trim().to_string(),
            number: index + 1,
        });
    }
    lines
}

fn is_closer(text: &str) -> bool {
    matches!(
        text.trim().to_lowercase().as_str(),
        "ende" | "end" | "}" | "einde" | "fin"
    )
}

pub fn parse(code: &str, language_code: &str, platform: &str) -> Result<Vec<Vec<BlockSpec>>, String> {
    let catalog = catalog::catalog();
    let language = catalog
        .languages
        .get(language_code)
        .ok_or_else(|| format!("nepo: unknown language '{language_code}'"))?;

    let lines = scan(code);
    let mut index = 0usize;
    let blocks = parse_stack(&lines, &mut index, 0, language, platform)?;

    if index < lines.len() {
        return Err(format!(
            "nepo: line {}: '{}' is indented less than the block it follows.",
            lines[index].number, lines[index].text
        ));
    }

    // Every top-level stack is one script. A Start block always begins a new
    // one; anything before it is a loose stack, which teaching material uses
    // to show a single block on its own.
    let mut scripts: Vec<Vec<BlockSpec>> = Vec::new();
    for block in blocks {
        let starts_script = block.shape == "start" || scripts.is_empty();
        if starts_script {
            scripts.push(vec![block]);
        } else {
            scripts.last_mut().expect("script").push(block);
        }
    }

    Ok(scripts)
}

fn parse_stack(
    lines: &[Line],
    index: &mut usize,
    indent: usize,
    language: &Language,
    platform: &str,
) -> Result<Vec<BlockSpec>, String> {
    let mut blocks = Vec::new();

    while *index < lines.len() {
        let line = &lines[*index];
        if line.indent < indent {
            break;
        }
        if is_closer(&line.text) {
            *index += 1;
            if line.indent < indent {
                break;
            }
            continue;
        }

        *index += 1;
        let (id, mut bindings) = match_line(&line.text, language, platform)
            .ok_or_else(|| unknown_line_error(line, language, platform))?;

        // Anything indented further belongs in this block's mouth.
        let child_indent = lines
            .get(*index)
            .filter(|next| next.indent > line.indent)
            .map(|next| next.indent);
        let statement_field = statement_field_of(&id, language);
        let mut continuation = Vec::new();
        if let Some(child_indent) = child_indent {
            let nested = parse_stack(lines, index, child_indent, language, platform)?;
            match &statement_field {
                // A block with a mouth swallows what is indented under it.
                Some(name) => {
                    bindings.insert(name.clone(), Binding::Statement(nested));
                }
                // Everything else reads the indent as "these follow me", which
                // is how the Start block is written in teaching material even
                // though NEPO gives it no mouth.
                None => continuation = nested,
            }
        }

        blocks.push(catalog::build_block(&id, language, &bindings)?);
        blocks.extend(continuation);
    }

    Ok(blocks)
}

fn unknown_line_error(line: &Line, language: &Language, platform: &str) -> String {
    let mut known: Vec<&str> = language
        .specs
        .keys()
        .filter(|id| catalog::supports(id, platform))
        .map(|id| id.as_str())
        .collect();
    known.sort_unstable();
    format!(
        "nepo: line {}: no block on platform '{}' matches '{}'. Known blocks: {}.",
        line.number,
        platform,
        line.text,
        known.join(", ")
    )
}

fn statement_field_of(id: &str, language: &Language) -> Option<String> {
    let def = catalog::catalog().blocks.get(id)?;
    let pattern = language.specs.get(id)?;
    pattern.fields().into_iter().find_map(|name| {
        def.fields
            .get(name)
            .filter(|field| field.kind == "statement")
            .map(|_| name.to_string())
    })
}

/// Tokens a source line is expected to supply: everything except the rows that
/// only exist to label a mouth.
fn parse_tokens<'a>(pattern: &'a Pattern, id: &str) -> Vec<&'a Token> {
    let def = catalog::catalog().blocks.get(id);
    pattern
        .rows
        .iter()
        .filter(|row| {
            !row.iter().any(|token| match token {
                Token::Field(name) => def
                    .and_then(|def| def.fields.get(name))
                    .map(|field| field.kind == "statement")
                    .unwrap_or(false),
                _ => false,
            })
        })
        .flatten()
        .collect()
}

fn candidates<'a>(language: &'a Language, platform: &str) -> Vec<(&'a str, &'a Pattern, bool)> {
    let mut out: Vec<(&str, &Pattern, bool)> = Vec::new();
    for (id, pattern) in &language.specs {
        if catalog::supports(id, platform) {
            out.push((id.as_str(), pattern, true));
        }
    }
    for (id, pattern) in &language.aliases {
        if catalog::supports(id, platform) {
            out.push((id.as_str(), pattern, false));
        }
    }
    out
}

/// Match a whole line. Ties are broken towards the canonical wording.
fn match_line(
    text: &str,
    language: &Language,
    platform: &str,
) -> Option<(String, HashMap<String, Binding>)> {
    let mut best: Option<(usize, bool, String, HashMap<String, Binding>)> = None;

    for (id, pattern, canonical) in candidates(language, platform) {
        let tokens = parse_tokens(pattern, id);
        let mut bindings = HashMap::new();
        if let Some(consumed) = match_tokens(&tokens, text, 0, id, language, platform, &mut bindings)
        {
            if text[consumed..].trim().is_empty() {
                let score = tokens.len();
                let better = match &best {
                    None => true,
                    Some((best_score, best_canonical, _, _)) => {
                        (canonical, score) > (*best_canonical, *best_score)
                    }
                };
                if better {
                    best = Some((score, canonical, id.to_string(), bindings));
                }
            }
        }
    }

    best.map(|(_, _, id, bindings)| (id, bindings))
}

fn skip_space(text: &str, mut pos: usize) -> usize {
    while let Some(ch) = text[pos..].chars().next() {
        if ch.is_whitespace() {
            pos += ch.len_utf8();
        } else {
            break;
        }
    }
    pos
}

fn match_tokens(
    tokens: &[&Token],
    text: &str,
    start: usize,
    id: &str,
    language: &Language,
    platform: &str,
    bindings: &mut HashMap<String, Binding>,
) -> Option<usize> {
    let def = catalog::catalog().blocks.get(id)?;
    let mut pos = start;

    for token in tokens {
        pos = skip_space(text, pos);
        match token {
            Token::Literal(literal) => {
                pos = match_literal(text, pos, literal)?;
            }
            Token::Field(name) => {
                let field = def.fields.get(name)?;
                let matched = match field.kind.as_str() {
                    "dropdown" => match_dropdown(text, pos, id, name, language),
                    "text" => match_text(text, pos),
                    "colour" => match_colour(text, pos),
                    "matrix" => match_matrix(text, pos),
                    "value" => match_value(text, pos, language, platform),
                    _ => return None,
                };
                match matched {
                    Some((binding, consumed)) => {
                        bindings.insert(name.clone(), binding);
                        pos = consumed;
                    }
                    // An empty socket is a legitimate thing to write: it is how
                    // a worksheet shows which type belongs in a hole. Every
                    // other field kind has to match.
                    None if field.kind == "value" => {}
                    None => return None,
                }
            }
        }
    }

    Some(pos)
}

/// Case-insensitive literal match that is safe on multi-byte characters and
/// forgiving about how much whitespace the author typed.
///
/// Comparing `text[pos..literal.len()]` would be neither: it panics in the
/// middle of a "ü", and it insists on exactly one space where the pattern has
/// one.
fn match_literal(text: &str, pos: usize, literal: &str) -> Option<usize> {
    let mut cursor = pos;
    let mut expected = literal.chars().peekable();

    while let Some(want) = expected.next() {
        if want.is_whitespace() {
            let skipped = skip_space(text, cursor);
            if skipped == cursor {
                return None;
            }
            cursor = skipped;
            while expected.peek().map(|ch| ch.is_whitespace()).unwrap_or(false) {
                expected.next();
            }
            continue;
        }
        let got = text[cursor..].chars().next()?;
        if !got.eq_ignore_ascii_case(&want) && got.to_lowercase().ne(want.to_lowercase()) {
            return None;
        }
        cursor += got.len_utf8();
    }

    Some(cursor)
}

fn match_dropdown(
    text: &str,
    pos: usize,
    id: &str,
    field: &str,
    language: &Language,
) -> Option<(Binding, usize)> {
    let mut options: Vec<&String> = language.dropdown_options(id, field).iter().collect();
    // Longest first, so "A+B" wins over "A".
    options.sort_by_key(|option| std::cmp::Reverse(option.chars().count()));
    for option in options {
        if let Some(end) = match_literal(text, pos, option) {
            return Some((Binding::Dropdown((*option).clone()), end));
        }
    }
    None
}

/// A quoted or bracketed literal: `"Hallo"`, `'Hallo'` or `[Hallo]`.
fn match_text(text: &str, pos: usize) -> Option<(Binding, usize)> {
    let rest = &text[pos..];
    let (open, close) = match rest.chars().next()? {
        '"' => ('"', '"'),
        '\'' => ('\'', '\''),
        '[' => ('[', ']'),
        _ => return None,
    };
    let inner_start = pos + open.len_utf8();
    let end = text[inner_start..].find(close)? + inner_start;
    Some((
        Binding::Text(text[inner_start..end].to_string()),
        end + close.len_utf8(),
    ))
}

fn match_colour(text: &str, pos: usize) -> Option<(Binding, usize)> {
    let rest = &text[pos..];
    if !rest.starts_with('#') {
        return None;
    }
    let digits: String = rest[1..].chars().take(6).collect();
    if digits.len() != 6 || !digits.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some((Binding::Colour(format!("#{digits}")), pos + 7))
}

/// `.#.#.|.#.#.|.....|#...#|.###.` — one string per LED row.
fn match_matrix(text: &str, pos: usize) -> Option<(Binding, usize)> {
    let rest = &text[pos..];
    let end = rest
        .find(|ch: char| !matches!(ch, '.' | '#' | '|' | '/' | ' ') && !ch.is_ascii_digit())
        .unwrap_or(rest.len());
    let body = &rest[..end];
    let rows: Vec<String> = body
        .split(['|', '/'])
        .map(|row| row.trim().to_string())
        .collect();
    if rows.len() != crate::matrix::SIZE {
        return None;
    }
    if rows.iter().any(|row| row.chars().count() != crate::matrix::SIZE) {
        return None;
    }
    Some((Binding::Matrix(rows), pos + end))
}

/// Parse a reporting block at this position, optionally in parentheses.
fn match_value(
    text: &str,
    pos: usize,
    language: &Language,
    platform: &str,
) -> Option<(Binding, usize)> {
    let rest = &text[pos..];

    if rest.starts_with('(') {
        let close = find_closing(rest)?;
        let inner = &rest[1..close];
        let (block, consumed) = match_value_body(inner, 0, language, platform)?;
        if inner[consumed..].trim().is_empty() {
            return Some((Binding::Value(block), pos + close + 1));
        }
        return None;
    }

    let (block, consumed) = match_value_body(text, pos, language, platform)?;
    Some((Binding::Value(block), consumed))
}

fn find_closing(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn match_value_body(
    text: &str,
    pos: usize,
    language: &Language,
    platform: &str,
) -> Option<(BlockSpec, usize)> {
    let mut best: Option<(usize, BlockSpec)> = None;

    for (id, pattern, _) in candidates(language, platform) {
        let def = catalog::catalog().blocks.get(id)?;
        if def.shape != "value" {
            continue;
        }
        let tokens = parse_tokens(pattern, id);
        let mut bindings = HashMap::new();
        if let Some(consumed) =
            match_tokens(&tokens, text, pos, id, language, platform, &mut bindings)
        {
            let longest = best.as_ref().map(|(best, _)| consumed > *best).unwrap_or(true);
            if longest {
                if let Ok(block) = catalog::build_block(id, language, &bindings) {
                    best = Some((consumed, block));
                }
            }
        }
    }

    best.map(|(consumed, block)| (block, consumed))
}

pub fn render_request(input: &str) -> Result<String, String> {
    let request: Request = serde_json::from_str(input)
        .map_err(|err| format!("nepo: invalid render request: {err}"))?;
    let scripts = parse(&request.code, &request.language, &request.platform)?;

    let document = DocumentSpec {
        dialect: crate::model::Dialect::Nepo,
        scale: request.scale,
        theme: request.theme,
        font: if request.font.is_empty() {
            "Helvetica Neue, Helvetica, Arial, sans-serif".to_string()
        } else {
            request.font
        },
        scripts: scripts
            .into_iter()
            .map(|blocks| ScriptSpec { blocks })
            .collect(),
    };

    if let Some(widths) = request.widths {
        if !widths.is_empty() {
            crate::measure::set_widths(widths);
            let svg = render_document(&document);
            crate::measure::clear_widths();
            return Ok(svg);
        }
    }
    Ok(render_document(&document))
}

/// Every string the renderer will paint, so the host can measure them first.
pub fn extract_texts(input: &str) -> Result<String, String> {
    let request: Request = serde_json::from_str(input)
        .map_err(|err| format!("nepo: invalid extract request: {err}"))?;
    let scripts = parse(&request.code, &request.language, &request.platform)?;
    let mut texts = std::collections::BTreeSet::new();
    for blocks in &scripts {
        let mut expanded = blocks.clone();
        for block in expanded.iter_mut() {
            crate::matrix::expand(block);
        }
        crate::measure::collect_texts(&expanded, &mut texts);
    }
    let list: Vec<String> = texts.into_iter().collect();
    serde_json::to_string(&list).map_err(|err| format!("nepo: failed to encode texts: {err}"))
}

/// The parsed structure, for tests and for callers that want the AST.
pub fn parse_request(input: &str) -> Result<String, String> {
    let request: Request = serde_json::from_str(input)
        .map_err(|err| format!("nepo: invalid parse request: {err}"))?;
    let scripts = parse(&request.code, &request.language, &request.platform)?;
    serde_json::to_string(&crate::debug::describe(&scripts))
        .map_err(|err| format!("nepo: failed to encode parse result: {err}"))
}
