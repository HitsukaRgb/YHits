use crate::app::Message;
use crate::download;
use crate::i18n::{Tr, t};
use iced::widget::text;
use iced::{Color, Element, Theme};

pub fn caption<'a>(content: impl iced::widget::text::IntoFragment<'a> + 'a) -> iced::widget::Text<'a> {
    text(content).size(13).style(secondary_text)
}

pub fn hint<'a>(content: &'a str, error: bool) -> iced::widget::Text<'a> {
    text(content).size(12).style(move |theme: &Theme| {
        let color = if error {
            theme.palette().danger
        } else {
            Color::from_rgb8(158, 150, 178)
        };
        iced::widget::text::Style { color: Some(color) }
    })
}

pub fn secondary_text(_theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(Color::from_rgb8(158, 150, 178)),
    }
}

pub fn link_hint(url: &str) -> Option<&'static str> {
    let url = url.trim();
    if url.is_empty() || download::is_youtube_url(url) {
        None
    } else {
        Some(t(Tr::HintInvalid))
    }
}

pub fn panel_style(_theme: &Theme) -> iced::widget::container::Style {
    use iced::border::Radius;
    use iced::{Background, Border};

    iced::widget::container::Style {
        background: Some(Background::Color(Color::from_rgb8(36, 32, 50))),
        border: Border {
            color: Color::from_rgba8(185, 163, 255, 0.16),
            width: 1.0,
            radius: Radius::new(12.0),
        },
        ..iced::widget::container::Style::default()
    }
}

#[derive(Clone, Copy)]
pub enum TagKind {
    Artist,
    Album,
}

pub fn tag_chip(label: &str, kind: TagKind) -> Element<'_, Message> {
    use iced::border::Border;
    use iced::widget::container;
    use iced::{Length, Padding};

    let (background, text_color, border, radius) = match kind {
        TagKind::Artist => (
            Color::from_rgba8(64, 186, 188, 0.34),
            Color::from_rgb8(198, 242, 240),
            Color::from_rgba8(64, 186, 188, 0.72),
            5.0,
        ),
        TagKind::Album => (
            Color::from_rgba8(220, 168, 96, 0.34),
            Color::from_rgb8(255, 236, 210),
            Color::from_rgba8(220, 168, 96, 0.72),
            8.0,
        ),
    };

    container(
        text(label.to_string())
            .size(10)
            .color(text_color)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .height(Length::Fixed(20.0))
    .padding(Padding::from([0.0, 8.0]))
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center)
    .style(move |_theme| container::Style {
        background: Some(background.into()),
        border: Border {
            color: border,
            width: 1.0,
            radius: radius.into(),
        },
        ..container::Style::default()
    })
    .into()
}

pub fn duration_chip(label: String) -> Element<'static, Message> {
    use iced::border::Border;
    use iced::widget::container;
    use iced::{Length, Padding};

    container(text(label).size(11).color(Color::from_rgb8(28, 22, 40)))
        .height(Length::Fixed(22.0))
        .padding(Padding::from([0.0, 12.0]))
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::Alignment::Center)
        .style(|_theme| container::Style {
            background: Some(Color::from_rgb8(255, 179, 199).into()),
            border: Border {
                color: Color::from_rgba8(28, 22, 40, 0.35),
                width: 1.0,
                radius: 20.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}
