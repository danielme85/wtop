use ratatui::style::Color;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Deserialize)]
struct ThemeFileDef {
    name: String,
    #[allow(dead_code)]
    description: Option<String>,
    colors: ThemeColors,
}

#[derive(Deserialize)]
struct ThemeColors {
    border: String,
    title: String,
    text: String,
    bg: String,
    running: String,
    stopped: String,
    dim: String,
    cyan: String,
    purple: String,
    accent: String,
}

/// Complete color theme for the TUI.
#[derive(Clone)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub border: Color,
    pub title: Color,
    pub text: Color,
    pub bg: Color,
    pub running: Color,
    pub stopped: Color,
    pub dim: Color,
    pub cyan: Color,
    pub purple: Color,
    pub accent: Color,
}

/// Built-in themes embedded in the binary. Written to disk on first run if absent.
const BUILTINS: &[(&str, &str)] = &[
    ("norse",  include_str!("../themes/norse.toml")),
    ("light",  include_str!("../themes/light.toml")),
    ("dark",   include_str!("../themes/dark.toml")),
    ("mono",   include_str!("../themes/mono.toml")),
    ("matrix", include_str!("../themes/matrix.toml")),
    ("c64",    include_str!("../themes/c64.toml")),
];

fn themes_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("wtop").join("themes"))
}

fn parse_hex(hex: &str) -> Color {
    let h = hex.trim_start_matches('#');
    if h.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&h[0..2], 16),
            u8::from_str_radix(&h[2..4], 16),
            u8::from_str_radix(&h[4..6], 16),
        ) {
            return Color::Rgb(r, g, b);
        }
    }
    Color::Reset
}

fn parse_theme(id: &str, content: &str) -> Option<Theme> {
    let def: ThemeFileDef = toml::from_str(content).ok()?;
    Some(Theme {
        id:      id.to_string(),
        name:    def.name,
        border:  parse_hex(&def.colors.border),
        title:   parse_hex(&def.colors.title),
        text:    parse_hex(&def.colors.text),
        bg:      parse_hex(&def.colors.bg),
        running: parse_hex(&def.colors.running),
        stopped: parse_hex(&def.colors.stopped),
        dim:     parse_hex(&def.colors.dim),
        cyan:    parse_hex(&def.colors.cyan),
        purple:  parse_hex(&def.colors.purple),
        accent:  parse_hex(&def.colors.accent),
    })
}

fn write_builtins_if_missing() {
    let dir = match themes_dir() {
        Some(d) => d,
        None => return,
    };
    let _ = std::fs::create_dir_all(&dir);
    for (id, content) in BUILTINS {
        let path = dir.join(format!("{}.toml", id));
        if !path.exists() {
            let _ = std::fs::write(path, content);
        }
    }
}

/// Load all themes: built-ins first (in fixed order), then user-added themes from
/// `~/.config/wtop/themes/`, sorted alphabetically. Built-in files are written to
/// the themes directory on first run so users can inspect and copy them.
pub fn load_all() -> Vec<Theme> {
    write_builtins_if_missing();

    let mut themes: Vec<Theme> = BUILTINS
        .iter()
        .filter_map(|(id, content)| parse_theme(id, content))
        .collect();

    let builtin_ids: HashSet<&str> = BUILTINS.iter().map(|(id, _)| *id).collect();

    if let Some(dir) = themes_dir() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut user: Vec<Theme> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "toml").unwrap_or(false))
                .filter_map(|e| {
                    let path = e.path();
                    let id = path.file_stem()?.to_str()?.to_string();
                    if builtin_ids.contains(id.as_str()) {
                        return None;
                    }
                    let content = std::fs::read_to_string(&path).ok()?;
                    parse_theme(&id, &content)
                })
                .collect();
            user.sort_by(|a, b| a.name.cmp(&b.name));
            themes.extend(user);
        }
    }

    themes
}

/// Find a theme by ID (case-insensitive). Falls back to the first theme if not found.
pub fn find_by_id<'a>(themes: &'a [Theme], id: &str) -> &'a Theme {
    themes
        .iter()
        .find(|t| t.id.eq_ignore_ascii_case(id))
        .or_else(|| themes.first())
        .expect("theme list is never empty")
}
