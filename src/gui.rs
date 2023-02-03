use std::sync::Arc;

use kludgine::core::figures::Points;
use kludgine::prelude::*;
use parking_lot::Mutex;

use crate::scrollback::Scrollback;
use crate::wrap::Wrapped;
use crate::{ConsoleCommand, ConsoleEvent, ConsoleHandle, State};

#[cfg(feature = "bundled-font")]
pub fn bundled_font() -> &'static Font {
    use once_cell::sync::OnceCell;
    static BUNDLED_FONT: OnceCell<Font> = OnceCell::new();

    BUNDLED_FONT.get_or_init(|| include_font!("../bundled-font/mononoki-Regular.ttf"))
}

pub(crate) fn run(console: ConsoleHandle) -> ! {
    let scrollback = Arc::new(Mutex::new(Scrollback::default()));
    SingleWindowApplication::run(Gui {
        zoom: 1.0,
        scrollback,
        console,
        line_height: Figure::new(0.),
        maximum_scroll: 0,
    })
}

pub struct Gui {
    zoom: f32,
    scrollback: Arc<Mutex<Scrollback>>,
    console: ConsoleHandle,
    maximum_scroll: usize,
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
        let commands = self.console.commands.clone();
        let scrollback = self.scrollback.clone();
        let state = self.console.state.clone();
        std::thread::spawn(move || command_handler(redrawer, commands, state, scrollback));
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
                    let mut input = self.console.state.input.lock();
                    if !input.suggestion.is_empty() {
                        let input = &mut *input;
                        input.buffer.push_str(&input.suggestion);
                        input.suggestion.clear();
                        status.set_needs_redraw();
                        self.console.send(ConsoleEvent::InputBufferChanged);
                    }
                }
                _ => {}
            },
            Event::MouseWheel { delta, .. } => {
                let mut scrollback = self.scrollback.lock();
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pixels) => {
                        let line_height = self.line_height.to_pixels(scene.scale());
                        pixels.y as f32 / line_height.get()
                    }
                };
                if lines > 0. {
                    scrollback.scroll = scrollback
                        .scroll
                        .saturating_add(lines as usize)
                        .min(self.maximum_scroll);
                } else {
                    scrollback.scroll = scrollback.scroll.saturating_sub((-lines) as usize);
                }
                status.set_needs_redraw();
            }
            _ => {}
        }

        Ok(())
    }

    fn receive_character(
        &mut self,
        ch: char,
        status: &mut RedrawStatus,
        scene: &Target,
        _window: WindowHandle,
    ) -> kludgine::app::Result<()>
    where
        Self: Sized,
    {
        if scene.modifiers_pressed().primary_modifier() {
            // This is a shortcut of some sort.
        } else {
            let mut input = self.console.state.input.lock();
            match ch {
                '\u{8}' => {
                    input.buffer.pop();
                    input.suggestion.clear();
                    self.console.send(ConsoleEvent::InputBufferChanged);
                }
                '\r' | '\n' => {
                    self.console.send(ConsoleEvent::Input);
                }
                '\t' => {}
                _ => {
                    input.buffer.push(ch);
                    if input.suggestion.starts_with(ch) {
                        input.suggestion.remove(0);
                    }
                    self.console.send(ConsoleEvent::InputBufferChanged);
                }
            }
            status.set_needs_redraw();
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
        let mut scrollback = self.scrollback.lock();
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

        input.buffer.rewrap(cols);
        let input_lines = input.buffer.lines();
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

            if line_number == input_lines_count - 1 && !input.suggestion.is_empty() {
                let suggestion = Text::prepare(
                    &input.suggestion,
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

        self.maximum_scroll = total_lines.saturating_sub(rows.saturating_sub(input_lines_count));
        if scrollback.scroll > self.maximum_scroll {
            // Oops, we were scrolled too far now that we've re-rendered.
            scrollback.scroll = self.maximum_scroll;
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

fn command_handler(
    redrawer: RedrawRequester,
    commands: flume::Receiver<ConsoleCommand>,
    state: Arc<State>,
    scrollback: Arc<Mutex<Scrollback>>,
) {
    while let Ok(command) = commands.recv() {
        match command {
            ConsoleCommand::Push(line) => {
                let mut scrollback = scrollback.lock();
                let mut wrapped = Wrapped::from(line);
                if scrollback.scroll != 0 {
                    // When the view port is scrolled, keep it at the same position
                    wrapped.rewrap(scrollback.columns);
                    let line_count = wrapped.lines().len();
                    scrollback.scroll += line_count;
                }
                scrollback.events.push_front(wrapped);
                redrawer.request_redraw();
            }
            ConsoleCommand::SetSuggestion(suggestion) => {
                let mut input = state.input.lock();
                input.suggestion = suggestion;
                redrawer.request_redraw();
            }
            ConsoleCommand::ResetInput => {
                let mut input = state.input.lock();
                input.buffer.clear();
                redrawer.request_redraw();
            }
            ConsoleCommand::ResetScroll => {
                let mut scrollback = scrollback.lock();
                scrollback.scroll = 0;
                redrawer.request_redraw();
            }
            ConsoleCommand::ResetScrollback => {
                let mut scrollback = scrollback.lock();
                scrollback.scroll = 0;
                scrollback.events.clear();
                redrawer.request_redraw();
            }
            ConsoleCommand::Shutdown => {
                state.shutdown();
            }
        }
    }
}
