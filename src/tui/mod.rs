pub mod app;
mod editor;
mod keymap;
mod render;

pub use app::run_tui;

#[cfg(test)]
mod tests;
