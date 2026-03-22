use ratatui::style::Color;

use crate::settings::ThemeName;

/// Complete color theme for the TUI.
pub struct Theme {
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

impl Theme {
    pub fn from_name(name: ThemeName) -> Self {
        match name {
            ThemeName::Norse => Self::norse(),
            ThemeName::Light => Self::light(),
            ThemeName::Dark => Self::dark(),
            ThemeName::Mono => Self::mono(),
        }
    }

    fn norse() -> Self {
        Self {
            border: Color::Rgb(30, 60, 114),
            title: Color::Rgb(218, 165, 32),
            text: Color::Rgb(189, 195, 199),
            bg: Color::Rgb(0, 0, 0),
            running: Color::Rgb(80, 200, 120),
            stopped: Color::Rgb(231, 76, 60),
            dim: Color::Rgb(100, 100, 120),
            cyan: Color::Rgb(52, 152, 219),
            purple: Color::Rgb(155, 89, 182),
            accent: Color::Rgb(218, 165, 32),
        }
    }

    fn light() -> Self {
        Self {
            border: Color::Rgb(50, 50, 200),
            title: Color::Rgb(30, 30, 30),
            text: Color::Rgb(40, 40, 40),
            bg: Color::Rgb(245, 245, 245),
            running: Color::Rgb(0, 150, 50),
            stopped: Color::Rgb(200, 50, 50),
            dim: Color::Rgb(140, 140, 140),
            cyan: Color::Rgb(0, 120, 180),
            purple: Color::Rgb(130, 60, 160),
            accent: Color::Rgb(50, 50, 200),
        }
    }

    fn dark() -> Self {
        Self {
            border: Color::Rgb(70, 90, 140),
            title: Color::Rgb(180, 140, 60),
            text: Color::Rgb(160, 165, 170),
            bg: Color::Rgb(20, 20, 30),
            running: Color::Rgb(60, 180, 100),
            stopped: Color::Rgb(200, 70, 55),
            dim: Color::Rgb(80, 80, 100),
            cyan: Color::Rgb(40, 130, 190),
            purple: Color::Rgb(140, 75, 165),
            accent: Color::Rgb(180, 140, 60),
        }
    }

    fn mono() -> Self {
        Self {
            border: Color::Gray,
            title: Color::White,
            text: Color::Gray,
            bg: Color::Black,
            running: Color::White,
            stopped: Color::DarkGray,
            dim: Color::DarkGray,
            cyan: Color::Gray,
            purple: Color::Gray,
            accent: Color::White,
        }
    }
}
