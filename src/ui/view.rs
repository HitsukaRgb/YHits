use super::bounds::notify_bounds;
use super::common::{hint, link_hint, panel_style};
use super::preview_list::preview_panel;
use super::tools::{language_capsule, tool_capsule, tool_popup};
use crate::app::{App, Message};
use crate::constants::{APP_TITLE, FORMAT_PICKER_WIDTH};
use crate::download::{AudioFormat, ToolKind};
use crate::i18n::{Tr, t, tf};
use crate::style;
use iced::widget::{
    button, column, container, float, mouse_area, progress_bar, row, stack, text, text_input,
};
use iced::{Alignment, Element, Length, Padding, Rectangle, Vector, mouse};

pub fn view(app: &App) -> Element<'_, Message> {
    let busy = app.control.is_some();
    let path = text_input(r"C:\Music\YHits", &app.output_dir)
        .on_input(Message::OutputDirChanged)
        .padding([8, 12])
        .size(13)
        .style(style::input_style)
        .width(Length::Fill);

    let url = text_input(t(Tr::UrlPlaceholder), &app.url)
        .on_input(Message::UrlChanged)
        .on_submit_maybe(app.download_message())
        .padding([8, 12])
        .size(13)
        .style(style::input_style)
        .width(Length::Fill);

    let action = if busy {
        button(text(t(Tr::Cancel)).size(13))
            .padding([8, 16])
            .style(style::danger_button_style)
            .on_press(Message::Cancel)
    } else {
        button(text(app.download_label()).size(13))
            .padding([8, 16])
            .style(style::accent_button_style)
            .on_press_maybe(app.download_message())
    };

    let retry = app.retry_items.as_ref().map(|items| {
        let label = if items.len() == 1 {
            t(Tr::Retry).into()
        } else {
            tf(Tr::RetryCount, &[("count", &items.len().to_string())])
        };
        button(text(label).size(13))
            .padding([8, 16])
            .style(style::accent_button_style)
            .on_press(Message::RetryFailed)
    });

    let header = row![
        text(APP_TITLE).size(26),
        iced::widget::Space::new().width(Length::Fill),
        language_capsule(app.language_code()),
        tool_capsule(ToolKind::YtDlp, app.tools.as_ref()),
        tool_capsule(ToolKind::Ffmpeg, app.tools.as_ref()),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let mut content = column![
        header,
        row![
            path,
            button(text(t(Tr::Browse)).size(13))
                .padding([8, 14])
                .style(style::button_style)
                .on_press(Message::Browse),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        {
            let mut url_block = column![url].spacing(4);
            if let Some(text) = link_hint(&app.url) {
                url_block = url_block.push(hint(text, true));
            }
            url_block
        },
        preview_panel(app),
    ]
    .spacing(12)
    .width(Length::Fill)
    .height(Length::Fill)
    .max_width(860);

    let mut footer = column![].spacing(10).width(Length::Fill);
    if let Some(progress) = &app.progress {
        footer = footer.push(
            row![
                text(format!("{} / {}", progress.done, progress.total))
                    .size(13)
                    .style(super::common::secondary_text),
                progress_bar(0.0..=100.0, progress.overall)
                    .girth(8)
                    .style(progress_bar::primary),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
    }

    let mut actions = row![iced::widget::Space::new().width(Length::Fill)]
        .spacing(8)
        .align_y(Alignment::Center);
    if let Some(retry) = retry {
        actions = actions.push(retry);
    }
    actions = actions
        .push(workers_control(app.download_workers, busy))
        .push(enrich_toggle(app.enrich_preview_meta))
        .push(format_trigger(app.audio_format, busy))
        .push(action);
    footer = footer.push(actions);
    content = content.push(footer);

    let page = container(content)
        .padding([24, 28])
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Top);

    let mut root = stack![page].width(Length::Fill).height(Length::Fill);
    if app.format_menu_open && app.tool_popup.is_none() {
        root = root.push(format_menu_overlay(
            app.audio_format,
            app.format_trigger_bounds,
        ));
    }
    if let Some(kind) = app.tool_popup {
        root = root.push(tool_popup(app, kind));
    }
    root.into()
}

fn workers_control(workers: usize, busy: bool) -> Element<'static, Message> {
    let label = tf(Tr::WorkersLabel, &[("count", &workers.to_string())]);
    row![
        button(text("−").size(13))
            .padding([8, 12])
            .style(style::button_style)
            .on_press_maybe((!busy && workers > crate::constants::MIN_DOWNLOAD_WORKERS).then_some(
                Message::DecreaseDownloadWorkers,
            )),
        container(text(label).size(13))
            .padding([8, 4])
            .align_y(Alignment::Center),
        button(text("+").size(13))
            .padding([8, 12])
            .style(style::button_style)
            .on_press_maybe((!busy && workers < crate::constants::MAX_DOWNLOAD_WORKERS).then_some(
                Message::IncreaseDownloadWorkers,
            )),
    ]
    .spacing(2)
    .align_y(Alignment::Center)
    .into()
}

fn enrich_toggle(enabled: bool) -> Element<'static, Message> {
    let style_fn = if enabled {
        style::accent_button_style
    } else {
        style::button_style
    };
    button(text(t(Tr::EnrichPreviewMeta)).size(13))
        .padding([8, 12])
        .style(style_fn)
        .on_press(Message::ToggleEnrichPreviewMeta)
        .into()
}

fn format_trigger(current: AudioFormat, busy: bool) -> Element<'static, Message> {
    let label = tf(Tr::FormatLabel, &[("format", current.label())]);
    let trigger = button(
        container(text(label).size(13))
            .width(Length::Fill)
            .align_x(Alignment::Center),
    )
    .padding([8, 0])
    .width(Length::Fixed(FORMAT_PICKER_WIDTH))
    .style(style::button_style)
    .on_press_maybe((!busy).then_some(Message::ToggleFormatMenu));

    notify_bounds(trigger, Message::FormatTriggerBounds)
}

fn format_menu_overlay(current: AudioFormat, anchor: Rectangle) -> Element<'static, Message> {
    const OPTION_HEIGHT: f32 = 34.0;
    const GAP: f32 = 6.0;

    let mut options = column![].spacing(4).width(Length::Fill);
    for format in AudioFormat::ALL {
        let selected = format == current;
        let style_fn = if selected {
            style::accent_button_style
        } else {
            style::button_style
        };

        let mut label = stack![
            container(text(format.label()).size(13))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        ]
        .width(Length::Fill)
        .height(Length::Fill);

        if selected {
            label = label.push(
                container(text("✓").size(13).style(style::accent_text))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(Padding::from([0.0, 10.0]))
                    .align_x(Alignment::End)
                    .align_y(Alignment::Center),
            );
        }

        options = options.push(
            button(label)
                .style(style_fn)
                .padding(0)
                .width(Length::Fill)
                .height(Length::Fixed(OPTION_HEIGHT))
                .on_press(Message::SetAudioFormat(format)),
        );
    }

    let menu_content = container(options)
        .padding(8)
        .width(Length::Fixed(FORMAT_PICKER_WIDTH))
        .style(panel_style);

    let backdrop = mouse_area(
        container(iced::widget::Space::new())
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .interaction(mouse::Interaction::Idle)
    .on_press(Message::CloseFormatMenu);

    let menu = float(
        mouse_area(menu_content)
            .interaction(mouse::Interaction::Idle)
            .on_press(Message::Noop),
    )
    .translate(move |bounds, viewport| {
        let max_x = (viewport.width - bounds.width).max(0.0);
        let max_y = (viewport.height - bounds.height).max(0.0);
        let x = (anchor.x + anchor.width - bounds.width).clamp(0.0, max_x);
        let y = (anchor.y - bounds.height - GAP).clamp(0.0, max_y);
        Vector::new(x - bounds.x, y - bounds.y)
    });

    stack![
        backdrop,
        container(menu)
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
