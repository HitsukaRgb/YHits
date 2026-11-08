#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod i18n;

mod app;
mod config;
mod constants;
mod download;
mod preview;
mod scroll;
mod style;
mod ui;

use app::{App, theme};
use constants::{APP_ICON, APP_TITLE};
use iced::{Font, Size, window};

fn main() -> iced::Result {
    iced::application(App::boot, App::update, ui::view)
        .title(APP_TITLE)
        .window(window_settings())
        .theme(theme)
        .default_font(Font::with_name("Segoe UI"))
        .run()
}

fn load_app_icon() -> Option<window::Icon> {
    ::image::load_from_memory(APP_ICON).ok().and_then(|img| {
        let rgba = img.into_rgba8();
        let (width, height) = rgba.dimensions();
        window::icon::from_rgba(rgba.into_raw(), width, height).ok()
    })
}

fn window_settings() -> window::Settings {
    window::Settings {
        size: Size::new(880.0, 760.0),
        min_size: Some(Size::new(700.0, 560.0)),
        position: window::Position::Centered,
        icon: load_app_icon(),
        ..Default::default()
    }
}
