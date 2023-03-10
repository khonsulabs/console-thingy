use std::ops::{Deref, DerefMut};

use kludgine::core::figures::Points;
use kludgine::prelude::*;

use crate::wrap::Wrapped;
use crate::{ConsoleHandle, InputMode};

#[cfg(feature = "bundled-font")]
pub fn bundled_font() -> &'static Font {
    use once_cell::sync::OnceCell;
    static BUNDLED_FONT: OnceCell<Font> = OnceCell::new();

    BUNDLED_FONT.get_or_init(|| include_font!("../bundled-font/mononoki-Regular.ttf"))
}

pub(crate) fn run(console: ConsoleHandle) -> ! {
    SingleWindowApplication::run(Gui {
        zoom: 1.0,
        console,
        line_height: Figure::new(0.),
    })
}

pub struct Gui {
    zoom: f32,
    console: ConsoleHandle,
    line_height: Figure<f32, Scaled>,
}

impl WindowCreator for Gui {
    fn window_title(&self) -> String {
        String::from("console-thingy")
    }
}

impl Window for Gui {
    fn initialize(
        &mut self,
        _scene: &Target,
        redrawer: RedrawRequester,
        _window: WindowHandle,
    ) -> kludgine::app::Result<()>
    where
        Self: Sized,
    {
        self.console
            .state
            .set_redrawer(move || redrawer.request_redraw());

        Ok(())
    }

    fn process_input(
        &mut self,
        input: InputEvent,
        status: &mut RedrawStatus,
        scene: &Target,
        _window: WindowHandle,
    ) -> kludgine::app::Result<()>
    where
        Self: Sized,
    {
        match input.event {
            Event::Keyboard {
                key: Some(key),
                state: ElementState::Pressed,
                ..
            } => match key {
                VirtualKeyCode::Plus | VirtualKeyCode::NumpadAdd
                    if scene.modifiers_pressed().primary_modifier() =>
                {
                    self.zoom += 0.1;
                    status.set_needs_redraw();
                }
                VirtualKeyCode::Minus | VirtualKeyCode::NumpadSubtract
                    if scene.modifiers_pressed().primary_modifier() =>
                {
                    self.zoom -= 0.1;
                    status.set_needs_redraw();
                }
                VirtualKeyCode::Numpad0 | VirtualKeyCode::Key0
                    if scene.modifiers_pressed().primary_modifier() =>
                {
                    self.zoom = 1.0;
                    status.set_needs_redraw();
                }
                VirtualKeyCode::Tab | VirtualKeyCode::Right => {
                    self.console.complete_suggestion();
                }
                _ => {}
            },
            Event::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pixels) => {
                        let line_height = self.line_height.to_pixels(scene.scale());
                        pixels.y as f32 / line_height.get()
                    }
                };
                self.console.scroll(lines as isize);
            }
            _ => {}
        }

        Ok(())
    }

    fn receive_character(
        &mut self,
        ch: char,
        _status: &mut RedrawStatus,
        scene: &Target,
        _window: WindowHandle,
    ) -> kludgine::app::Result<()>
    where
        Self: Sized,
    {
        if scene.modifiers_pressed().primary_modifier() {
            // This is a shortcut of some sort.
        } else {
            self.console.input(ch);
        }
        Ok(())
    }

    fn render(
        &mut self,
        scene: &Target,
        status: &mut RedrawStatus,
        _window: WindowHandle,
    ) -> kludgine::app::Result<()> {
        let mut input = self.console.state.input.lock();
        let input = &mut *input;
        let mut scrollback = self.console.state.scrollback.lock();
        let one_char = Text::prepare(
            "m",
            &self.console.state.config.font,
            Figure::new(14.0),
            Color::WHITE,
            scene,
        );
        let one_char_width = one_char.width;
        let cols = (scene.size().width() / one_char_width.to_scaled(scene.scale())).get() as usize;
        scrollback.columns = cols;
        let ascent = Figure::<f32, Pixels>::new(one_char.metrics.ascent).to_scaled(scene.scale());
        let descent = Figure::<f32, Pixels>::new(one_char.metrics.descent).to_scaled(scene.scale());
        let line_height = ascent - descent;
        let rows = (scene.size().height() / line_height).get() as usize;

        let mut input_source = match &mut input.mode {
            InputMode::Text | InputMode::Suggesting(_) => {
                WrappedSource::Borrowed(&mut input.buffer)
            }
            InputMode::Secure => {
                WrappedSource::Owned(Wrapped::from("*".repeat(input.buffer.len())))
            }
        };
        input_source.rewrap(cols);
        let input_lines = input_source.lines();
        let input_lines_count = input_lines.len();

        let input_top = scene.size().height() + descent - line_height * input_lines_count as f32;
        Shape::rect(Rect::new(
            Point::from_figures(Figure::new(0.), input_top),
            Size::from_figures(scene.size().width(), Figure::new(1.)),
        ))
        .fill(Fill::new(Color::WHITE))
        .render(scene);

        let mut baseline = input_top + ascent;
        for (line_number, line) in input_lines.enumerate() {
            let prepared = Text::prepare(
                line,
                &self.console.state.config.font,
                Figure::new(14.0),
                Color::WHITE,
                scene,
            );
            prepared.render_baseline_at(scene, Point::from_figures(Figure::new(0.), baseline))?;

            if line_number == input_lines_count - 1 {
                if let InputMode::Suggesting(suggestion) = &input.mode {
                    let suggestion = Text::prepare(
                        suggestion,
                        &self.console.state.config.font,
                        Figure::new(14.0),
                        Color::GRAY,
                        scene,
                    );
                    suggestion.render_baseline_at(
                        scene,
                        Point::from_figures(prepared.width.to_scaled(scene.scale()), baseline),
                    )?;
                }
            }
            baseline += line_height;
        }

        let mut y = input_top + descent;
        let mut total_lines = 0;
        let scroll = scrollback.scroll;
        for line in &mut scrollback.events {
            line.rewrap(cols);
            let lines = line.lines();

            for line in lines.rev() {
                total_lines += 1;
                if total_lines <= scroll {
                    continue;
                }
                let prepared = Text::prepare(
                    line,
                    &self.console.state.config.font,
                    Figure::new(14.0),
                    Color::WHITE,
                    scene,
                );
                prepared.render_baseline_at(scene, Point::from_figures(Figure::new(0.), y))?;
                y -= line_height;
            }
        }

        scrollback.maximum_scroll =
            total_lines.saturating_sub(rows.saturating_sub(input_lines_count));
        if scrollback.scroll > scrollback.maximum_scroll {
            // Oops, we were scrolled too far now that we've re-rendered.
            scrollback.scroll = scrollback.maximum_scroll;
            status.set_needs_redraw();
        }

        Ok(())
    }

    fn update(
        &mut self,
        _scene: &Target,
        _status: &mut RedrawStatus,
        window: WindowHandle,
    ) -> kludgine::app::Result<()>
    where
        Self: Sized,
    {
        if self.console.should_shutdown() {
            self.console
                .shutdown()
                .map_err(kludgine::app::Error::Other)?;
            window.request_close();
        }
        Ok(())
    }

    fn additional_scale(&self) -> Scale<f32, Scaled, Points> {
        Scale::new(self.zoom * 2.)
    }
}

enum WrappedSource<'a> {
    Borrowed(&'a mut Wrapped),
    Owned(Wrapped),
}

impl<'a> Deref for WrappedSource<'a> {
    type Target = Wrapped;

    fn deref(&self) -> &Self::Target {
        match self {
            WrappedSource::Borrowed(wrapped) => wrapped,
            WrappedSource::Owned(wrapped) => wrapped,
        }
    }
}

impl<'a> DerefMut for WrappedSource<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            WrappedSource::Borrowed(wrapped) => wrapped,
            WrappedSource::Owned(wrapped) => wrapped,
        }
    }
}
