use super::common::{caption, panel_style, secondary_text};
use crate::app::{App, Message};
use crate::constants::{
    LANG_CAPSULE_WIDTH, LOG_FONT, TOOL_CAPSULE_HEIGHT, TOOL_CAPSULE_WIDTH, TOOL_POPUP_DETAIL_HEIGHT,
    TOOL_POPUP_WIDTH,
};
use crate::download::{ToolKind, ToolStatus};
use crate::i18n::{Tr, t, tf};
use crate::style;
use iced::widget::{button, column, container, mouse_area, row, stack, text};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Theme, mouse,
};

pub fn language_capsule(code: &'static str) -> Element<'static, Message> {
    let background = Color::from_rgba8(185, 163, 255, 0.18);
    let text_color = Color::from_rgb8(220, 212, 242);
    let border = Color::from_rgba8(185, 163, 255, 0.55);

    let chip = container(
        text(code)
            .size(12)
            .color(text_color)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fixed(LANG_CAPSULE_WIDTH))
    .height(Length::Fixed(TOOL_CAPSULE_HEIGHT))
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(Alignment::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            color: border,
            width: 1.0,
            radius: 20.0.into(),
        },
        ..container::Style::default()
    });

    mouse_area(
        button(chip)
            .padding(0)
            .style(move |_theme, _status| button::Style {
                background: None,
                text_color,
                border: Border::default(),
                shadow: Default::default(),
                snap: false,
            })
            .on_press(Message::CycleLanguage),
    )
    .interaction(mouse::Interaction::Pointer)
    .into()
}

pub fn tool_capsule(kind: ToolKind, tools: Option<&ToolStatus>) -> Element<'static, Message> {
    let present = tools.map(|status| status.has(kind));
    let (background, text_color, border) = match present {
        Some(true) => (
            Color::from_rgba8(72, 180, 120, 0.28),
            Color::from_rgb8(196, 242, 214),
            Color::from_rgba8(72, 180, 120, 0.85),
        ),
        Some(false) => (
            Color::from_rgba8(232, 93, 106, 0.28),
            Color::from_rgb8(255, 206, 212),
            Color::from_rgba8(232, 93, 106, 0.85),
        ),
        None => (
            Color::from_rgba8(158, 150, 178, 0.18),
            Color::from_rgb8(186, 178, 204),
            Color::from_rgba8(158, 150, 178, 0.45),
        ),
    };

    let chip = container(
        text(kind.label())
            .size(12)
            .color(text_color)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fixed(TOOL_CAPSULE_WIDTH))
    .height(Length::Fixed(TOOL_CAPSULE_HEIGHT))
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(Alignment::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            color: border,
            width: 1.0,
            radius: 20.0.into(),
        },
        ..container::Style::default()
    });

    mouse_area(
        button(chip)
            .padding(0)
            .style(move |_theme, _status| button::Style {
                background: None,
                text_color,
                border: Border::default(),
                shadow: Default::default(),
                snap: false,
            })
            .on_press(Message::OpenToolPopup(kind)),
    )
    .interaction(mouse::Interaction::Pointer)
    .into()
}

pub fn tool_popup(app: &App, kind: ToolKind) -> Element<'_, Message> {
    let installing = app.tool_installing == Some(kind);
    let checking = app.checking_tools && !installing;
    let version = app.tools.as_ref().and_then(|status| status.version(kind));
    let known = app.tools.is_some();
    let present = version.is_some();
    let missing = known && !present;
    let busy = installing || checking;

    let detail_body: Element<'_, Message> = if installing {
        caption(t(Tr::ToolInstalling)).into()
    } else if checking && !known {
        caption(t(Tr::ToolCheckingPresence)).into()
    } else if checking {
        if let Some(version) = version {
            column![
                caption(t(Tr::ToolChecking)),
                text(tf(Tr::ToolVersion, &[("version", version)]))
                    .font(LOG_FONT)
                    .size(13)
                    .style(secondary_text),
            ]
            .spacing(6)
            .into()
        } else {
            column![
                caption(t(Tr::ToolChecking)),
                text(t(Tr::ToolMissing)).size(13).style(|theme: &Theme| {
                    iced::widget::text::Style {
                        color: Some(theme.palette().danger),
                    }
                }),
            ]
            .spacing(6)
            .into()
        }
    } else if let Some(version) = version {
        column![
            caption(t(Tr::ToolInstalled)),
            text(tf(Tr::ToolVersion, &[("version", version)]))
                .font(LOG_FONT)
                .size(13)
                .style(secondary_text),
        ]
        .spacing(6)
        .into()
    } else {
        let mut body = column![
            text(t(Tr::ToolMissing)).size(13).style(|theme: &Theme| {
                iced::widget::text::Style {
                    color: Some(theme.palette().danger),
                }
            }),
            caption(t(Tr::ToolInstallHint)),
            text(format!("winget install --id {} -e", kind.winget_id()))
                .font(LOG_FONT)
                .size(11)
                .style(secondary_text),
        ]
        .spacing(6);
        if let Some(error) = &app.tool_install_error {
            body = body.push(text(error.clone()).size(12).style(|theme: &Theme| {
                iced::widget::text::Style {
                    color: Some(theme.palette().danger),
                }
            }));
        }
        body.into()
    };

    let detail = container(detail_body)
        .width(Length::Fill)
        .height(Length::Fixed(TOOL_POPUP_DETAIL_HEIGHT));

    let download = button(text(t(Tr::Download)).size(13))
        .padding([8, 14])
        .style(style::accent_button_style)
        .on_press_maybe((missing && !busy).then_some(Message::InstallTool(kind)));
    let recheck = button(text(t(Tr::Check)).size(13))
        .padding([8, 14])
        .style(style::button_style)
        .on_press_maybe((!busy).then_some(Message::RecheckTools));
    let close = button(text(t(Tr::Close)).size(13))
        .padding([8, 14])
        .style(style::button_style)
        .on_press_maybe((!busy).then_some(Message::CloseToolPopup));

    let mut actions = row![].spacing(8).align_y(Alignment::Center);
    if missing || (checking && known && !present) {
        actions = actions.push(download);
    }
    actions = actions
        .push(recheck)
        .push(iced::widget::Space::new().width(Length::Fill))
        .push(close);

    let panel = container(
        column![
            text(kind.label())
                .size(18)
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center),
            detail,
            actions,
        ]
        .spacing(14)
        .width(Length::Fill),
    )
    .padding(20)
    .width(Length::Fixed(TOOL_POPUP_WIDTH))
    .style(panel_style);

    let backdrop_layer = container(iced::widget::Space::new())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.45))),
            ..container::Style::default()
        });
    let backdrop = if busy {
        mouse_area(backdrop_layer).interaction(mouse::Interaction::Idle)
    } else {
        mouse_area(backdrop_layer)
            .interaction(mouse::Interaction::Idle)
            .on_press(Message::CloseToolPopup)
    };

    let modal = container(
        mouse_area(panel)
            .interaction(mouse::Interaction::Idle)
            .on_press(Message::Noop),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(Alignment::Center);

    stack![backdrop, modal]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
