use iced::widget::{button, container, scrollable, text_input};
use iced::widget::scrollable::{AutoScroll, Rail, Scroller};
use iced::{Background, Border, Color, Shadow, Theme, border::Radius};

pub mod colors {
    use iced::Color;

    pub const PRIMARY_TEXT: Color = Color::from_rgb8(238, 234, 248);
    pub const SECONDARY_TEXT: Color = Color::from_rgb8(158, 150, 178);
    pub const SECONDARY_TEXT_DIM: Color = Color::from_rgb8(120, 114, 138);
    pub const PRIMARY_BG: Color = Color::from_rgb8(36, 32, 50);
    pub const SECONDARY_BG: Color = Color::from_rgb8(18, 16, 28);
    pub const ACCENT: Color = Color::from_rgb8(185, 163, 255);
    pub const ACCENT_SOFT: Color = Color::from_rgba8(185, 163, 255, 0.55);
    pub const ACCENT_BORDER: Color = Color::from_rgba8(185, 163, 255, 0.34);
    pub const EDGE: Color = Color::from_rgb8(74, 66, 100);
    pub const EDGE_SOFT: Color = Color::from_rgba8(74, 66, 100, 0.55);
    pub const EDGE_DIM: Color = Color::from_rgba8(74, 66, 100, 0.35);
    pub const HOVER_WASH: Color = Color::from_rgba8(255, 255, 255, 0.06);
    pub const PRESS_WASH: Color = Color::from_rgba8(255, 255, 255, 0.12);
    pub const DANGER: Color = Color::from_rgb8(232, 93, 106);
    pub const SELECTION: Color = Color::from_rgba8(185, 163, 255, 0.35);
    pub const PRIMARY_TEXT_MUTED: Color = Color::from_rgb8(185, 178, 205);
    pub const SCROLLER_RAIL: Color = Color::from_rgba8(185, 163, 255, 0.26);
    pub const SCROLLER_BORDER: Color = Color::from_rgba8(18, 16, 28, 0.55);
    pub const AUTO_SCROLL_BORDER: Color = Color::from_rgba8(185, 163, 255, 0.42);
    pub const BRIGHT_FILL: Color = Color::from_rgb8(248, 247, 255);
    pub const PRIMARY_BG_FAINT: Color = Color::from_rgb8(44, 40, 60);
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
}

fn input_well(depth: f32) -> Color {
    mix(colors::PRIMARY_BG, colors::SECONDARY_BG, depth)
}

pub fn input_style(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let (background, border_color, text_color, placeholder_color, border_width) = match status {
        text_input::Status::Active => (
            input_well(0.38),
            colors::EDGE_SOFT,
            colors::PRIMARY_TEXT,
            colors::SECONDARY_TEXT_DIM,
            1.0,
        ),
        text_input::Status::Hovered => (
            input_well(0.28),
            colors::ACCENT_SOFT,
            colors::PRIMARY_TEXT,
            colors::SECONDARY_TEXT_DIM,
            1.0,
        ),
        text_input::Status::Focused { .. } => (
            input_well(0.32),
            colors::ACCENT,
            colors::PRIMARY_TEXT,
            colors::SECONDARY_TEXT_DIM,
            1.5,
        ),
        text_input::Status::Disabled => (
            input_well(0.22),
            colors::EDGE_DIM,
            colors::SECONDARY_TEXT_DIM,
            colors::SECONDARY_TEXT_DIM,
            1.0,
        ),
    };

    text_input::Style {
        background: Background::Color(background),
        border: Border {
            color: border_color,
            width: border_width,
            radius: Radius::new(10.0),
        },
        icon: colors::SECONDARY_TEXT,
        placeholder: placeholder_color,
        value: text_color,
        selection: colors::SELECTION,
    }
}

pub fn button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => colors::HOVER_WASH,
        button::Status::Pressed => colors::PRESS_WASH,
        button::Status::Disabled => Color::TRANSPARENT,
        _ => Color::TRANSPARENT,
    };
    let text_color = match status {
        button::Status::Disabled => colors::SECONDARY_TEXT,
        _ => colors::PRIMARY_TEXT,
    };
    let border_color = match status {
        button::Status::Disabled => colors::EDGE,
        _ => colors::ACCENT_BORDER,
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            color: border_color,
            width: 1.0,
            radius: Radius::new(10.0),
        },
        ..Default::default()
    }
}

pub fn accent_text(_theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(colors::ACCENT),
    }
}

pub fn accent_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => colors::HOVER_WASH,
        button::Status::Pressed => colors::PRESS_WASH,
        button::Status::Disabled => Color::TRANSPARENT,
        _ => Color::TRANSPARENT,
    };
    let text_color = match status {
        button::Status::Disabled => colors::SECONDARY_TEXT,
        _ => colors::ACCENT,
    };
    let border_color = match status {
        button::Status::Disabled => colors::EDGE,
        _ => colors::ACCENT,
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            color: border_color,
            width: 1.5,
            radius: Radius::new(10.0),
        },
        ..Default::default()
    }
}

pub fn scrollable_style(_theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let is_hovered = matches!(
        status,
        scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. }
    );

    let scroller_color = if is_hovered {
        colors::PRIMARY_TEXT
    } else {
        colors::PRIMARY_TEXT_MUTED
    };

    let rail = Rail {
        background: Some(Background::Color(colors::SCROLLER_RAIL)),
        border: Border {
            radius: 5.0.into(),
            ..Default::default()
        },
        scroller: Scroller {
            background: Background::Color(scroller_color),
            border: Border {
                radius: 5.0.into(),
                width: 1.0,
                color: colors::SCROLLER_BORDER,
            },
        },
    };

    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: AutoScroll {
            background: Background::Color(colors::PRIMARY_BG_FAINT),
            border: Border {
                radius: 50.0.into(),
                width: 1.0,
                color: colors::AUTO_SCROLL_BORDER,
            },
            shadow: Shadow::default(),
            icon: colors::BRIGHT_FILL,
        },
    }
}

pub fn danger_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let danger = colors::DANGER;
    let soft = Color {
        a: danger.a * 0.14,
        ..danger
    };
    let soft_pressed = Color {
        a: danger.a * 0.22,
        ..danger
    };
    let background = match status {
        button::Status::Hovered => soft,
        button::Status::Pressed => soft_pressed,
        button::Status::Disabled => Color::TRANSPARENT,
        _ => Color::TRANSPARENT,
    };
    let text_color = match status {
        button::Status::Disabled => colors::SECONDARY_TEXT,
        _ => danger,
    };
    let border_color = match status {
        button::Status::Disabled => colors::EDGE,
        _ => danger,
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            color: border_color,
            width: 1.0,
            radius: Radius::new(10.0),
        },
        ..Default::default()
    }
}
