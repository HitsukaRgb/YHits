use super::common::{TagKind, caption, duration_chip, panel_style, secondary_text, tag_chip};
use crate::app::{App, Message, PreviewState};
use crate::constants::{COVER_RADIUS, COVER_SIZE, ROW_HEIGHT, ROW_SPACING};
use crate::i18n::{Tr, t, tf};
use crate::preview::TrackPreview;
use crate::scroll::{self, ListScrollState};
use crate::style;
use iced::widget::{button, column, container, image, mouse_area, row, text};
use iced::{
    Border, Color, Element, Length, Padding, Theme, border::Radius, mouse,
};

pub fn preview_panel(app: &App) -> Element<'_, Message> {
    let busy = app.control.is_some();
    let header = match &app.preview {
        PreviewState::Ready(bundle) => {
            let selected = bundle.tracks.iter().filter(|track| track.selected).count();
            let total = bundle.tracks.len();
            let label = match &bundle.source_title {
                Some(title) if !title.is_empty() => {
                    format!("{title}  ·  {selected}/{total}")
                }
                _ => tf(
                    Tr::TracksCount,
                    &[
                        ("selected", &selected.to_string()),
                        ("total", &total.to_string()),
                    ],
                ),
            };
            row![
                text(label).size(13).style(secondary_text),
                iced::widget::Space::new().width(Length::Fill),
                button(text(t(Tr::SelectAll)).size(12))
                    .padding([4, 10])
                    .style(style::button_style)
                    .on_press_maybe((!busy).then_some(Message::SelectAll)),
                button(text(t(Tr::SelectNone)).size(12))
                    .padding([4, 10])
                    .style(style::button_style)
                    .on_press_maybe((!busy).then_some(Message::SelectNone)),
            ]
            .spacing(6)
            .align_y(iced::alignment::Vertical::Center)
        }
        _ => row![caption(t(Tr::Tracks))].into(),
    };

    let body: Element<'_, Message> = match &app.preview {
        PreviewState::Idle => container(caption(t(Tr::PreviewIdle)))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .into(),
        PreviewState::Loading => caption(t(Tr::PreviewLoading)).into(),
        PreviewState::Failed(message) => text(message.clone())
            .size(13)
            .style(|theme: &Theme| iced::widget::text::Style {
                color: Some(theme.palette().danger),
            })
            .into(),
        PreviewState::Ready(bundle) => {
            let total = bundle.tracks.len();
            let (start, end) = app.track_scroll.visible_range(
                total,
                ROW_HEIGHT,
                ROW_SPACING,
                scroll::LIST_OVERSCAN_BACK,
                scroll::LIST_OVERSCAN_AHEAD,
            );
            let rows = scroll::virtual_column(
                total,
                start,
                end,
                ROW_HEIGHT,
                ROW_SPACING,
                |index| track_row(index, &bundle.tracks[index], busy),
            );
            let list = container(rows)
                .padding(Padding {
                    top: 12.0,
                    right: scroll::RAIL_GUTTER,
                    bottom: 12.0,
                    left: 12.0,
                })
                .width(Length::Fill);
            let scroll_state = app
                .track_scroll
                .with_content_extent(total, ROW_HEIGHT, ROW_SPACING);
            scroll::vertical_min_thumb_scroll(
                list,
                scroll_state,
                |viewport| {
                    let state = ListScrollState::from_viewport_y(viewport);
                    Message::TracksScrolled {
                        offset: state.offset,
                        viewport: state.viewport_size,
                        content: state.content_size,
                    }
                },
                Message::ScrollTracksTo,
            )
        }
    };

    let panel_body: Element<'_, Message> = match &app.preview {
        PreviewState::Ready(_) => body,
        _ => container(body)
            .padding(12)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    };

    column![
        header,
        container(panel_body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(panel_style),
    ]
    .spacing(6)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn track_row(index: usize, track: &TrackPreview, busy: bool) -> Element<'_, Message> {
    let cover = track_cover(track);
    let mut tags = row![].spacing(4).align_y(iced::Alignment::Center);
    for artist in &track.artists {
        tags = tags.push(tag_chip(artist, TagKind::Artist));
    }
    if !track.album.trim().is_empty() {
        tags = tags.push(tag_chip(&track.album, TagKind::Album));
    }

    let meta = column![
        text(&track.title)
            .size(13)
            .wrapping(iced::widget::text::Wrapping::None)
            .width(Length::Fill),
        tags,
    ]
    .spacing(4)
    .width(Length::Fill);

    let mut content = row![cover, meta]
        .spacing(10)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(iced::Alignment::Center);

    if let Some(secs) = track.duration_secs {
        content = content.push(duration_chip(crate::preview::format_duration(secs)));
    }

    let selected = track.selected;
    let row = container(content)
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT))
        .padding(Padding::from([0.0, 10.0]))
        .align_y(iced::Alignment::Center)
        .style(move |_theme| track_row_style(selected));

    if busy {
        row.into()
    } else {
        mouse_area(row)
            .interaction(mouse::Interaction::Pointer)
            .on_press(Message::ToggleTrack(index))
            .into()
    }
}

fn track_cover(track: &TrackPreview) -> Element<'_, Message> {
    let dim = Length::Fixed(COVER_SIZE);
    let radius = Radius::new(COVER_RADIUS);

    let inner: Element<'_, Message> = if let Some(handle) = &track.cover {
        image(handle.clone())
            .width(dim)
            .height(dim)
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        container(text("♪").size(18).style(secondary_text))
            .width(dim)
            .height(dim)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .into()
    };

    container(inner)
        .width(dim)
        .height(dim)
        .clip(true)
        .style(move |_theme| container::Style {
            background: Some(Color::from_rgba8(90, 84, 112, 0.5).into()),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius,
            },
            ..container::Style::default()
        })
        .into()
}

fn track_row_style(selected: bool) -> container::Style {
    let (border_color, border_width, background) = if selected {
        (
            Color::from_rgb8(185, 163, 255),
            2.0,
            Some(Color::from_rgba8(185, 163, 255, 0.14).into()),
        )
    } else {
        (Color::from_rgba8(185, 163, 255, 0.28), 1.0, None)
    };

    container::Style {
        background,
        border: Border {
            color: border_color,
            width: border_width,
            radius: Radius::new(10.0),
        },
        ..container::Style::default()
    }
}
