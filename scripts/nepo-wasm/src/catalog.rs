//! The block catalog: structure from `data/blocks.toml`, text from
//! `data/locales/<lang>.toml`, both embedded at compile time.

use crate::model::{BlockSpec, RowSpec, SegmentSpec};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

const BLOCKS_TOML: &str = include_str!("../data/blocks.toml");

const LOCALES: &[(&str, &str)] = &[
    ("de", include_str!("../data/locales/de.toml")),
    ("en", include_str!("../data/locales/en.toml")),
];

#[derive(Debug, Deserialize)]
struct BlocksToml {
    blocks: HashMap<String, BlockDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockDef {
    #[serde(default)]
    pub platforms: Vec<String>,
    pub shape: String,
    pub category: String,
    /// Output data type for `value` blocks.
    #[serde(default)]
    pub check: Option<String>,
    #[serde(default)]
    pub fields: HashMap<String, FieldDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldDef {
    pub kind: String,
    #[serde(default)]
    pub check: Option<String>,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LocaleToml {
    #[serde(default)]
    dir: Option<String>,
    specs: HashMap<String, String>,
    #[serde(default)]
    dropdowns: HashMap<String, HashMap<String, Vec<String>>>,
    #[serde(default)]
    aliases: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Literal(String),
    Field(String),
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub rows: Vec<Vec<Token>>,
}

impl Pattern {
    fn compile(spec: &str) -> Pattern {
        let rows = spec
            .split('|')
            .map(|row| compile_row(row.trim()))
            .filter(|row| !row.is_empty())
            .collect();
        Pattern { rows }
    }

    /// Names of the placeholders in the pattern, in reading order.
    pub fn fields(&self) -> Vec<&str> {
        self.rows
            .iter()
            .flatten()
            .filter_map(|token| match token {
                Token::Field(name) => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }
}

fn compile_row(row: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut literal = String::new();
    let mut chars = row.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '%' {
            literal.push(ch);
            continue;
        }
        let mut name = String::new();
        while let Some(&next) = chars.peek() {
            if next.is_ascii_alphanumeric() || next == '_' {
                name.push(next);
                chars.next();
            } else {
                break;
            }
        }
        if name.is_empty() {
            literal.push('%');
            continue;
        }
        let trimmed = literal.trim();
        if !trimmed.is_empty() {
            tokens.push(Token::Literal(trimmed.to_string()));
        }
        literal.clear();
        tokens.push(Token::Field(name));
    }

    let trimmed = literal.trim();
    if !trimmed.is_empty() {
        tokens.push(Token::Literal(trimmed.to_string()));
    }
    tokens
}

#[derive(Debug)]
pub struct Language {
    pub code: String,
    pub rtl: bool,
    /// Canonical pattern per block, used for rendering.
    pub specs: HashMap<String, Pattern>,
    /// Extra patterns accepted when parsing, never rendered.
    pub aliases: Vec<(String, Pattern)>,
    /// block id -> field name -> options, first entry is the default.
    pub dropdowns: HashMap<String, HashMap<String, Vec<String>>>,
}

impl Language {
    pub fn dropdown_options(&self, block_id: &str, field: &str) -> &[String] {
        self.dropdowns
            .get(block_id)
            .and_then(|fields| fields.get(field))
            .map(|options| options.as_slice())
            .unwrap_or(&[])
    }
}

#[derive(Debug)]
pub struct Catalog {
    pub blocks: HashMap<String, BlockDef>,
    pub languages: HashMap<String, Language>,
}

static CATALOG: OnceLock<Catalog> = OnceLock::new();

pub fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(|| {
        let blocks: BlocksToml = toml::from_str(BLOCKS_TOML).expect("data/blocks.toml");
        let mut languages = HashMap::new();

        for (code, source) in LOCALES {
            let locale: LocaleToml =
                toml::from_str(source).unwrap_or_else(|err| panic!("locales/{code}.toml: {err}"));

            let specs: HashMap<String, Pattern> = locale
                .specs
                .iter()
                .map(|(id, spec)| (id.clone(), Pattern::compile(spec)))
                .collect();

            let mut aliases = Vec::new();
            for (id, patterns) in &locale.aliases {
                for spec in patterns {
                    aliases.push((id.clone(), Pattern::compile(spec)));
                }
            }

            languages.insert(
                code.to_string(),
                Language {
                    code: code.to_string(),
                    rtl: locale
                        .dir
                        .as_deref()
                        .map(|dir| dir.eq_ignore_ascii_case("rtl"))
                        .unwrap_or(false),
                    specs,
                    aliases,
                    dropdowns: locale.dropdowns,
                },
            );
        }

        Catalog { blocks: blocks.blocks, languages }
    })
}

/// What a parsed line supplies for one placeholder.
#[derive(Debug, Clone)]
pub enum Binding {
    Dropdown(String),
    Text(String),
    Colour(String),
    Matrix(Vec<String>),
    Value(BlockSpec),
    Statement(Vec<BlockSpec>),
}

/// Assemble a renderable block from the catalog plus what the parser found.
///
/// Rendering always uses the *canonical* pattern, never the alias that matched,
/// so a document written with a colloquial wording still prints Open Roberta's
/// own words.
pub fn build_block(
    id: &str,
    language: &Language,
    bindings: &HashMap<String, Binding>,
) -> Result<BlockSpec, String> {
    let catalog = catalog();
    let def = catalog
        .blocks
        .get(id)
        .ok_or_else(|| format!("nepo: unknown block '{id}'"))?;
    let pattern = language
        .specs
        .get(id)
        .ok_or_else(|| format!("nepo: block '{id}' has no {} pattern", language.code))?;

    let mut rows = Vec::new();

    for tokens in &pattern.rows {
        let mut fields: Vec<SegmentSpec> = Vec::new();
        let mut kind = "dummy".to_string();
        let mut check = None;
        let mut value = None;
        let mut body = Vec::new();

        for token in tokens {
            match token {
                Token::Literal(text) => fields.push(SegmentSpec::Text {
                    value: text.clone(),
                    monospace: false,
                }),
                Token::Field(name) => {
                    let field = def.fields.get(name).ok_or_else(|| {
                        format!("nepo: block '{id}' has no field '{name}' but its pattern uses it")
                    })?;
                    match field.kind.as_str() {
                        "dropdown" => {
                            let selected = match bindings.get(name) {
                                Some(Binding::Dropdown(text)) => text.clone(),
                                _ => language
                                    .dropdown_options(id, name)
                                    .first()
                                    .cloned()
                                    .unwrap_or_default(),
                            };
                            fields.push(SegmentSpec::Dropdown { value: selected });
                        }
                        "text" => {
                            let text = match bindings.get(name) {
                                Some(Binding::Text(text)) => text.clone(),
                                _ => field.default.clone().unwrap_or_default(),
                            };
                            fields.push(SegmentSpec::Input { value: text });
                        }
                        "colour" => {
                            let colour = match bindings.get(name) {
                                Some(Binding::Colour(colour)) => colour.clone(),
                                _ => field
                                    .default
                                    .clone()
                                    .unwrap_or_else(|| "#ff0000".to_string()),
                            };
                            fields.push(SegmentSpec::ColourField { colour });
                        }
                        "matrix" => {
                            let cells = match bindings.get(name) {
                                Some(Binding::Matrix(cells)) => cells.clone(),
                                _ => vec![String::new(); crate::matrix::SIZE],
                            };
                            fields.push(SegmentSpec::PixelMatrix { rows: cells });
                        }
                        "value" => {
                            kind = "value".to_string();
                            check = field.check.clone();
                            if let Some(Binding::Value(block)) = bindings.get(name) {
                                value = Some(Box::new(block.clone()));
                            }
                        }
                        "statement" => {
                            kind = "statement".to_string();
                            if let Some(Binding::Statement(blocks)) = bindings.get(name) {
                                body = blocks.clone();
                            }
                        }
                        other => {
                            return Err(format!(
                                "nepo: block '{id}' field '{name}' has unknown kind '{other}'"
                            ))
                        }
                    }
                }
            }
        }

        rows.push(RowSpec {
            kind,
            align: "left".to_string(),
            fields,
            check,
            value,
            body,
        });
    }

    Ok(BlockSpec {
        dialect: Some("nepo".to_string()),
        id: id.to_string(),
        shape: def.shape.clone(),
        category: def.category.clone(),
        check: def.check.clone(),
        segments: Vec::new(),
        rows,
    })
}

/// Is this block offered on the selected platform?
pub fn supports(id: &str, platform: &str) -> bool {
    catalog()
        .blocks
        .get(id)
        .map(|def| def.platforms.is_empty() || def.platforms.iter().any(|p| p == platform))
        .unwrap_or(false)
}
