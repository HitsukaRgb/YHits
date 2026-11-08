use crate::style;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::border::Border;
use iced::mouse;
use iced::widget::scrollable::{AbsoluteOffset, Direction, Scrollbar};
use iced::widget::{container, scrollable, stack};
use iced::{
    Alignment, Background, Element, Event, Length, Rectangle, Size, Task, Theme, window,
};

pub const TRACKS_SCROLL_ID: &str = "yt-audio-tracks-scroll";
pub const MIN_THUMB: f32 = 32.0;
pub const THUMB_THICKNESS: f32 = 6.0;
pub const RAIL_GUTTER: f32 = 14.0;
pub const LIST_OVERSCAN_BACK: usize = 2;
pub const LIST_OVERSCAN_AHEAD: usize = 6;
const MIN_USABLE_VIEWPORT: f32 = 32.0;
const DEFAULT_VIEWPORT_SIZE: f32 = 720.0;

#[derive(Debug, Clone, Copy)]
pub struct ListScrollState {
    pub offset: f32,
    pub viewport_size: f32,
    pub content_size: f32,
}

impl Default for ListScrollState {
    fn default() -> Self {
        Self {
            offset: 0.0,
            viewport_size: DEFAULT_VIEWPORT_SIZE,
            content_size: 0.0,
        }
    }
}

impl ListScrollState {
    pub fn apply_scroll_report(&mut self, offset: f32, viewport: f32, content: f32) {
        self.offset = offset.max(0.0);
        if viewport > MIN_USABLE_VIEWPORT {
            self.viewport_size = viewport;
            self.content_size = content.max(0.0);
        }
    }

    pub fn from_viewport_y(viewport: scrollable::Viewport) -> Self {
        Self {
            offset: viewport.absolute_offset().y.max(0.0),
            viewport_size: viewport.bounds().height.max(1.0),
            content_size: viewport.content_bounds().height.max(0.0),
        }
    }

    pub fn content_extent(item_count: usize, item_size: f32, spacing: f32) -> f32 {
        if item_count == 0 {
            0.0
        } else {
            item_count as f32 * item_size + (item_count - 1) as f32 * spacing
        }
    }

    pub fn with_content_extent(mut self, item_count: usize, item_size: f32, spacing: f32) -> Self {
        self.content_size = Self::content_extent(item_count, item_size, spacing);
        self
    }

    pub fn visible_range(
        &self,
        item_count: usize,
        item_size: f32,
        spacing: f32,
        back: usize,
        ahead: usize,
    ) -> (usize, usize) {
        if item_count == 0 {
            return (0, 0);
        }

        let stride = (item_size + spacing).max(1.0);
        let viewport = if self.viewport_size > 1.0 {
            self.viewport_size
        } else {
            DEFAULT_VIEWPORT_SIZE
        };
        let content_len = Self::content_extent(item_count, item_size, spacing);
        let max_offset = (content_len - viewport).max(0.0);
        let offset = self.offset.clamp(0.0, max_offset);

        let start = ((offset / stride).floor() as isize - back as isize).max(0) as usize;
        let visible = (viewport / stride).ceil() as usize + 1;
        let end = start
            .saturating_add(visible)
            .saturating_add(ahead)
            .min(item_count);
        (start.min(end), end)
    }
}

pub fn virtual_column<'a, Message: 'a>(
    total: usize,
    start: usize,
    end: usize,
    row_height: f32,
    spacing: f32,
    mut row_at: impl FnMut(usize) -> Element<'a, Message>,
) -> iced::widget::Column<'a, Message> {
    use iced::widget::{Space, column, container};

    let mut col = column![].spacing(0).width(Length::Fill);
    if total == 0 {
        return col;
    }

    let stride = row_height + spacing;
    let start = start.min(total);
    let end = end.min(total).max(start);

    col = col.push(Space::new().height(Length::Fixed(start as f32 * stride)));

    for index in start..end {
        let item = row_at(index);
        if index + 1 == total {
            col = col.push(item);
        } else {
            col = col.push(
                container(item)
                    .width(Length::Fill)
                    .padding(iced::Padding {
                        bottom: spacing,
                        ..iced::Padding::ZERO
                    }),
            );
        }
    }

    let bottom = if end < total {
        (total - end) as f32 * stride - spacing
    } else {
        0.0
    };
    col = col.push(Space::new().height(Length::Fixed(bottom.max(0.0))));
    col
}

pub fn scroll_to_y<Message: 'static>(offset_y: f32) -> Task<Message> {
    iced::widget::operation::scroll_to(
        TRACKS_SCROLL_ID,
        AbsoluteOffset {
            x: None,
            y: Some(offset_y.max(0.0)),
        },
    )
}

fn thumb_len(track: f32, viewport: f32, content: f32) -> f32 {
    if content <= viewport || content <= 0.0 || track <= 0.0 {
        return track;
    }
    (track * (viewport / content)).max(MIN_THUMB).min(track)
}

fn thumb_offset(track: f32, thumb: f32, offset: f32, max_offset: f32) -> f32 {
    if max_offset <= 0.0 || track <= thumb {
        0.0
    } else {
        (track - thumb) * (offset / max_offset).clamp(0.0, 1.0)
    }
}

fn offset_from_thumb(track: f32, thumb: f32, thumb_pos: f32, max_offset: f32) -> f32 {
    if track <= thumb || max_offset <= 0.0 {
        0.0
    } else {
        (thumb_pos / (track - thumb)).clamp(0.0, 1.0) * max_offset
    }
}

struct RailState {
    grab: Option<f32>,
    hovered: bool,
}

impl Default for RailState {
    fn default() -> Self {
        Self {
            grab: None,
            hovered: false,
        }
    }
}

struct MinThumbRail<'a, Message> {
    scroll: ListScrollState,
    on_scroll_to: Box<dyn Fn(f32) -> Message + 'a>,
}

impl<'a, Message> MinThumbRail<'a, Message> {
    fn new(scroll: ListScrollState, on_scroll_to: impl Fn(f32) -> Message + 'a) -> Self {
        Self {
            scroll,
            on_scroll_to: Box::new(on_scroll_to),
        }
    }

    fn layout_viewport(&self, bounds: Rectangle) -> f32 {
        bounds.height
    }

    fn max_offset(&self, bounds: Rectangle) -> f32 {
        (self.scroll.content_size - self.layout_viewport(bounds)).max(0.0)
    }

    fn needs_rail(&self, bounds: Rectangle) -> bool {
        let viewport = self.layout_viewport(bounds);
        viewport > MIN_USABLE_VIEWPORT && self.scroll.content_size > viewport + 0.5
    }

    fn thumb_rect(&self, bounds: Rectangle) -> Rectangle {
        let viewport = self.layout_viewport(bounds);
        let max_offset = self.max_offset(bounds);
        let thumb = thumb_len(bounds.height, viewport, self.scroll.content_size);
        let top = thumb_offset(bounds.height, thumb, self.scroll.offset, max_offset);
        let x = bounds.x + (bounds.width - THUMB_THICKNESS).max(0.0) * 0.5;
        Rectangle {
            x,
            y: bounds.y + top,
            width: THUMB_THICKNESS,
            height: thumb,
        }
    }

    fn rail_track(&self, bounds: Rectangle) -> Rectangle {
        let x = bounds.x + (bounds.width - THUMB_THICKNESS).max(0.0) * 0.5;
        Rectangle {
            x,
            y: bounds.y,
            width: THUMB_THICKNESS,
            height: bounds.height,
        }
    }

    fn publish_offset_at_cursor(
        &self,
        cursor: iced::Point,
        bounds: Rectangle,
        shell: &mut Shell<'_, Message>,
        grab: f32,
    ) {
        let viewport = self.layout_viewport(bounds);
        let max_offset = self.max_offset(bounds);
        let thumb = thumb_len(bounds.height, viewport, self.scroll.content_size);
        let top = (cursor.y - bounds.y - grab).clamp(0.0, (bounds.height - thumb).max(0.0));
        let offset = offset_from_thumb(bounds.height, thumb, top, max_offset);
        shell.publish((self.on_scroll_to)(offset));
    }
}

impl<'a, Message> Widget<Message, Theme, iced::Renderer> for MinThumbRail<'a, Message>
where
    Message: Clone,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<RailState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(RailState::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fixed(RAIL_GUTTER),
            height: Length::Fill,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, Length::Fixed(RAIL_GUTTER), Length::Fill)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if !self.needs_rail(bounds) {
            let state = tree.state.downcast_mut::<RailState>();
            state.grab = None;
            state.hovered = false;
            return;
        }

        let state = tree.state.downcast_mut::<RailState>();
        let thumb = self.thumb_rect(bounds);
        let over_rail = cursor.is_over(bounds);
        let over_thumb = cursor.position().is_some_and(|p| thumb.contains(p));

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor.position_over(bounds) {
                    let grab = if over_thumb {
                        pos.y - thumb.y
                    } else {
                        thumb.height * 0.5
                    };
                    state.grab = Some(grab);
                    self.publish_offset_at_cursor(pos, bounds, shell, grab);
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.grab.take().is_some() {
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let (Some(grab), Some(pos)) = (state.grab, cursor.position()) {
                    self.publish_offset_at_cursor(pos, bounds, shell, grab);
                    shell.capture_event();
                }
            }
            _ => {}
        }

        let hovered = over_rail || state.grab.is_some();
        if state.hovered != hovered {
            state.hovered = hovered;
            shell.request_redraw();
        } else if let Event::Window(window::Event::RedrawRequested(_)) = event {
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        if !self.needs_rail(bounds) {
            return mouse::Interaction::None;
        }
        let state = tree.state.downcast_ref::<RailState>();
        if state.grab.is_some() || cursor.is_over(bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;

        let bounds = layout.bounds();
        if !self.needs_rail(bounds) {
            return;
        }

        let state = tree.state.downcast_ref::<RailState>();
        let track = self.rail_track(bounds);
        let thumb = self.thumb_rect(bounds);

        let thumb_color = if state.grab.is_some() || state.hovered {
            style::colors::PRIMARY_TEXT
        } else {
            style::colors::PRIMARY_TEXT_MUTED
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds: track,
                border: Border {
                    radius: 5.0.into(),
                    ..Border::default()
                },
                ..renderer::Quad::default()
            },
            Background::Color(style::colors::SCROLLER_RAIL),
        );

        renderer.fill_quad(
            renderer::Quad {
                bounds: thumb,
                border: Border {
                    radius: 5.0.into(),
                    width: 1.0,
                    color: style::colors::SCROLLER_BORDER,
                },
                ..renderer::Quad::default()
            },
            Background::Color(thumb_color),
        );
    }
}

impl<'a, Message> From<MinThumbRail<'a, Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(rail: MinThumbRail<'a, Message>) -> Self {
        Element::new(rail)
    }
}

pub fn vertical_min_thumb_scroll<'a, Message: Clone + 'a>(
    content: impl Into<Element<'a, Message>>,
    scroll: ListScrollState,
    on_scroll: impl Fn(scrollable::Viewport) -> Message + 'a,
    on_scroll_to: impl Fn(f32) -> Message + 'a,
) -> Element<'a, Message> {
    let body = scrollable(content)
        .id(TRACKS_SCROLL_ID)
        .style(style::scrollable_style)
        .on_scroll(on_scroll)
        .direction(Direction::Vertical(Scrollbar::hidden()))
        .width(Length::Fill)
        .height(Length::Fill);

    stack![
        body,
        container(MinThumbRail::new(scroll, on_scroll_to))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
