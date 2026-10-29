//! Signal Desktop dark-theme tokens (from stylesheets/_variables.scss).

use iced::border;
use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Shadow, Theme};

pub const GRAY_02: Color = Color::from_rgb(246.0 / 255.0, 246.0 / 255.0, 246.0 / 255.0);
pub const GRAY_25: Color = Color::from_rgb(185.0 / 255.0, 185.0 / 255.0, 185.0 / 255.0);
pub const GRAY_45: Color = Color::from_rgb(132.0 / 255.0, 132.0 / 255.0, 132.0 / 255.0);
pub const GRAY_65: Color = Color::from_rgb(74.0 / 255.0, 74.0 / 255.0, 74.0 / 255.0);
pub const GRAY_75: Color = Color::from_rgb(59.0 / 255.0, 59.0 / 255.0, 59.0 / 255.0);
pub const GRAY_80: Color = Color::from_rgb(46.0 / 255.0, 46.0 / 255.0, 46.0 / 255.0);
pub const GRAY_85: Color = Color::from_rgb(38.0 / 255.0, 38.0 / 255.0, 38.0 / 255.0);
pub const GRAY_90: Color = Color::from_rgb(27.0 / 255.0, 27.0 / 255.0, 27.0 / 255.0);
pub const GRAY_95: Color = Color::from_rgb(18.0 / 255.0, 18.0 / 255.0, 18.0 / 255.0);

pub const ULTRAMARINE: Color = Color::from_rgb(44.0 / 255.0, 107.0 / 255.0, 237.0 / 255.0);
pub const ULTRAMARINE_LIGHT: Color = Color::from_rgb(97.0 / 255.0, 145.0 / 255.0, 243.0 / 255.0);
pub const ULTRAMARINE_DARK: Color = Color::from_rgb(24.0 / 255.0, 81.0 / 255.0, 180.0 / 255.0);
pub const DANGER: Color = Color::from_rgb(226.0 / 255.0, 69.0 / 255.0, 69.0 / 255.0);
pub const SUCCESS: Color = Color::from_rgb(74.0 / 255.0, 176.0 / 255.0, 101.0 / 255.0);
pub const STAMP_OUT: Color = Color {
    r: 1.0,
    g: 1.0,
    b: 1.0,
    a: 0.7,
};

/// Avatar backgrounds Signal cycles through for contacts without photos.
pub const AVATAR_COLORS: [Color; 8] = [
    Color::from_rgb(226.0 / 255.0, 101.0 / 255.0, 112.0 / 255.0),
    Color::from_rgb(243.0 / 255.0, 160.0 / 255.0, 99.0 / 255.0),
    Color::from_rgb(201.0 / 255.0, 168.0 / 255.0, 77.0 / 255.0),
    Color::from_rgb(97.0 / 255.0, 176.0 / 255.0, 134.0 / 255.0),
    Color::from_rgb(82.0 / 255.0, 163.0 / 255.0, 178.0 / 255.0),
    Color::from_rgb(97.0 / 255.0, 145.0 / 255.0, 243.0 / 255.0),
    Color::from_rgb(154.0 / 255.0, 112.0 / 255.0, 226.0 / 255.0),
    Color::from_rgb(226.0 / 255.0, 112.0 / 255.0, 178.0 / 255.0),
];

pub fn avatar_color(seed: u128) -> Color {
    AVATAR_COLORS[(seed as usize) % AVATAR_COLORS.len()]
}

pub fn solid(color: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(color)),
        text_color: Some(GRAY_02),
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

pub fn rounded(bg: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(GRAY_02),
        border: border::rounded(radius),
        shadow: Shadow::default(),
    }
}

pub fn sidebar(_theme: &Theme) -> container::Style {
    solid(GRAY_90)
}

/// Chat pane, header strip, and composer share the same fill.
pub fn pane(_theme: &Theme) -> container::Style {
    solid(GRAY_95)
}

/// Dim layer behind the link-device dialog.
pub fn link_dim(_theme: &Theme) -> container::Style {
    solid(Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.55,
    })
}

/// Black panel that holds link instructions and the QR code.
pub fn link_panel(_theme: &Theme) -> container::Style {
    solid(Color::BLACK)
}

pub fn call_banner(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(GRAY_85)),
        text_color: Some(GRAY_02),
        border: Border {
            color: GRAY_80,
            width: 1.0,
            radius: 0.0.into(),
        },
        shadow: Shadow::default(),
    }
}

pub fn bubble_out(_theme: &Theme) -> container::Style {
    rounded(ULTRAMARINE, 18.0)
}

pub fn bubble_in(_theme: &Theme) -> container::Style {
    rounded(GRAY_80, 18.0)
}

pub fn divider(_theme: &Theme) -> container::Style {
    solid(GRAY_80)
}

pub fn search_field(theme: &Theme, status: text_input::Status) -> text_input::Style {
    field(theme, status, GRAY_85)
}

pub fn composer_field(theme: &Theme, status: text_input::Status) -> text_input::Style {
    field(theme, status, GRAY_80)
}

fn field(_theme: &Theme, status: text_input::Status, bg: Color) -> text_input::Style {
    let border = match status {
        text_input::Status::Focused => Border {
            color: ULTRAMARINE,
            width: 1.5,
            radius: 18.0.into(),
        },
        text_input::Status::Hovered => Border {
            color: GRAY_65,
            width: 1.0,
            radius: 18.0.into(),
        },
        _ => Border {
            color: GRAY_75,
            width: 1.0,
            radius: 18.0.into(),
        },
    };
    text_input::Style {
        background: Background::Color(bg),
        border,
        icon: GRAY_45,
        placeholder: GRAY_45,
        value: GRAY_02,
        selection: ULTRAMARINE_DARK,
    }
}

pub fn conv_row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let bg = if selected {
            GRAY_80
        } else {
            match status {
                button::Status::Hovered | button::Status::Pressed => GRAY_85,
                _ => Color::TRANSPARENT,
            }
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: GRAY_02,
            border: border::rounded(8),
            shadow: Shadow::default(),
        }
    }
}

pub fn icon_btn(accent: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| {
        let (idle, hover, fg) = if accent {
            (ULTRAMARINE, ULTRAMARINE_LIGHT, Color::WHITE)
        } else {
            (GRAY_80, GRAY_75, GRAY_02)
        };
        filled(status, idle, hover, fg)
    }
}

pub fn ghost_btn(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => GRAY_80,
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: GRAY_25,
        border: border::rounded(8),
        shadow: Shadow::default(),
    }
}

pub fn danger_btn(_theme: &Theme, status: button::Status) -> button::Style {
    filled(
        status,
        DANGER,
        Color::from_rgb(200.0 / 255.0, 50.0 / 255.0, 50.0 / 255.0),
        Color::WHITE,
    )
}

pub fn success_btn(_theme: &Theme, status: button::Status) -> button::Style {
    filled(
        status,
        SUCCESS,
        Color::from_rgb(60.0 / 255.0, 150.0 / 255.0, 90.0 / 255.0),
        Color::WHITE,
    )
}

fn filled(status: button::Status, idle: Color, hover: Color, fg: Color) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => hover,
        _ => idle,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: border::rounded(18),
        shadow: Shadow::default(),
    }
}
