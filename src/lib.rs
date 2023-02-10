use parking_lot::Mutex;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::scrollback::Scrollback;
use crate::wrap::Wrapped;

#[cfg(feature = "gui")]
mod gui;
mod scrollback;
#[cfg(feature = "tui")]
mod tui;
mod wrap;

#[derive(Debug)]
pub struct Config {
    #[cfg(feature = "kludgine")]
    font: kludgine::core::text::Font,
}

#[cfg(feature = "bundled-font")]
impl Default for Config {
    fn default() -> Self {
        Self {
            font: gui::bundled_font().clone(),
        }
    }
}

#[cfg(not(feature = "kludgine"))]
impl Default for Config {
    fn default() -> Self {
        Self {}
    }
}

impl Config {
    #[cfg(all(feature = "gui", feature = "tui"))]
    pub fn run<T>(self, app: T) -> !
    where
        T: App,
    {
        let state = Arc::new(State::from(self));
        let console = Console::spawn(app, state);
        if tui::is_tty() {
            tui::run(console)
        } else {
            gui::run(console)
        }
    }

    #[cfg(all(feature = "gui", not(feature = "tui")))]
    pub fn run<T>(self, app: T) -> !
    where
        T: App,
    {
        let state = Arc::new(State::from(self));
        let (console, sender, receiver) = Console::spawn(app, state.clone());
        gui::run(state, thread, sender, receiver)
    }

    #[cfg(all(feature = "tui", not(feature = "gui")))]
    pub fn run<T>(self, app: T) -> !
    where
        T: App,
    {
        let (console, sender, receiver) = Console::new();
        let thread = spawn_app(app, console);
        tui::run(self, thread, sender, receiver)
    }
}

pub trait App: Send + 'static {
    fn run(self, console: Console) -> anyhow::Result<()>;
}

impl<T> App for T
where
    T: FnOnce(Console) -> anyhow::Result<()> + Send + 'static,
{
    fn run(self, console: Console) -> anyhow::Result<()> {
        self(console)
    }
}

fn spawn_app<T: App>(app: T, console: Console) -> JoinHandle<anyhow::Result<()>> {
    std::thread::Builder::new()
        .name(String::from("app"))
        .spawn(|| app_thread(app, console))
        .expect("error spawning app thread")
}

fn app_thread<T: App>(app: T, console: Console) -> anyhow::Result<()> {
    app.run(console)
}

#[derive(Clone)]
pub struct Console {
    state: Arc<State>,
    app: flume::Receiver<ConsoleEvent>,
}

impl Console {
    fn spawn<T: App>(app: T, state: Arc<State>) -> ConsoleHandle {
        let (app_sender, app_receiver) = flume::unbounded();
        let thread = spawn_app(
            app,
            Self {
                state: state.clone(),
                app: app_receiver,
            },
        );
        ConsoleHandle {
            state,
            thread: Some(thread),
            events: Some(app_sender),
        }
    }

    pub fn push_line(&self, line: impl Into<String>) {
        self.state.push(line.into());
        self.state.redraw();
    }

    pub fn set_suggestion(&self, suggestion: impl Into<String>) {
        self.state.set_suggestion(suggestion.into());
        self.state.redraw();
    }

    pub fn clear_secure(&self) {
        self.state.clear_secure();
        self.state.redraw();
    }

    pub fn set_secure(&self) {
        self.state.set_secure();
        self.state.redraw();
    }

    pub fn input(&self) -> Input {
        let input = self.state.input.lock();
        input.clone()
    }

    pub fn clear_input(&self) {
        self.state.clear_input();
        self.state.redraw();
    }

    pub fn clear_scrollback(&self) {
        self.state.clear_scrollback();
        self.state.redraw();
    }

    pub fn reset_scroll(&self) {
        self.state.scroll_to_current();
        self.state.redraw();
    }

    pub fn next_event(&self) -> Result<ConsoleEvent, flume::RecvError> {
        self.app.recv()
    }

    pub fn should_shutdown(&self) -> bool {
        self.state.should_shutdown()
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        // If this is the last reference, mark the state as being shut down.
        if Arc::strong_count(&self.state) == 1 {
            self.state.shutdown();
            self.state.redraw();
        }
    }
}

struct ConsoleHandle {
    state: Arc<State>,
    thread: Option<JoinHandle<anyhow::Result<()>>>,
    events: Option<flume::Sender<ConsoleEvent>>,
}

impl ConsoleHandle {
    pub fn should_shutdown(&self) -> bool {
        if self.state.should_shutdown() {
            true
        } else {
            self.thread.as_ref().map_or(true, JoinHandle::is_finished)
        }
    }

    pub fn shutdown(&mut self) -> anyhow::Result<()> {
        // Disconnect the thread, so that we can join the handle.
        self.state.shutdown();
        self.events = None;
        if let Some(thread) = self.thread.take() {
            thread.join().expect("console thread panicked")?;
        }

        Ok(())
    }

    pub fn send(&self, event: ConsoleEvent) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
    }

    pub fn input(&self, ch: char) {
        let mut input = self.state.input.lock();
        match ch {
            '\u{8}' => {
                input.buffer.pop();
                if let InputMode::Suggesting(suggestion) = &mut input.mode {
                    suggestion.clear();
                }

                self.send(ConsoleEvent::InputBufferChanged);
            }
            '\r' | '\n' => {
                self.send(ConsoleEvent::Input);
            }
            '\t' => {}
            _ => {
                input.buffer.push(ch);
                if let InputMode::Suggesting(suggestion) = &mut input.mode {
                    if suggestion.starts_with(ch) {
                        suggestion.remove(0);
                    }
                }
                self.send(ConsoleEvent::InputBufferChanged);
            }
        }
        self.state.redraw();
    }

    pub fn complete_suggestion(&self) -> bool {
        let mut input = self.state.input.lock();
        let input = &mut *input;

        if let InputMode::Suggesting(suggestion) = &mut input.mode {
            if suggestion.is_empty() {
                false
            } else {
                input.buffer.push_str(suggestion);
                suggestion.clear();
                self.state.redraw();
                self.send(ConsoleEvent::InputBufferChanged);
                true
            }
        } else {
            false
        }
    }

    pub fn scroll(&self, lines: isize) {
        let mut scrollback = self.state.scrollback.lock();
        if lines > 0 {
            scrollback.scroll = scrollback
                .scroll
                .saturating_add(lines as usize)
                .min(scrollback.maximum_scroll);
        } else if lines == isize::MIN {
            // Can't negate safely due to to being unrepresentable
            scrollback.scroll = scrollback.scroll.saturating_sub(isize::MAX as usize + 1);
        } else {
            scrollback.scroll = scrollback.scroll.saturating_sub((-lines) as usize);
        }
        self.state.redraw();
    }
}

pub enum ConsoleEvent {
    InputBufferChanged,
    Input,
}

struct State {
    config: Config,
    shutdown: Mutex<bool>,
    input: Mutex<Input>,
    scrollback: Mutex<Scrollback>,
    redrawer: Mutex<Option<Box<dyn Redrawer>>>,
}

impl From<Config> for State {
    fn from(config: Config) -> Self {
        Self {
            config,
            shutdown: Mutex::new(false),
            input: Mutex::default(),
            scrollback: Mutex::default(),
            redrawer: Mutex::default(),
        }
    }
}

impl State {
    pub fn should_shutdown(&self) -> bool {
        *self.shutdown.lock()
    }

    pub fn shutdown(&self) {
        *self.shutdown.lock() = true;
    }

    pub fn set_redrawer<R>(&self, redrawer: R)
    where
        R: Redrawer,
    {
        let mut installed = self.redrawer.lock();
        *installed = Some(Box::new(redrawer));
    }

    pub fn redraw(&self) {
        let mut redrawer = self.redrawer.lock();
        if let Some(redrawer) = &mut *redrawer {
            redrawer.redraw();
        }
    }

    pub fn push(&self, line: String) {
        let mut scrollback = self.scrollback.lock();
        let mut wrapped = Wrapped::from(line);
        if scrollback.scroll != 0 {
            // When the view port is scrolled, keep it at the same position
            wrapped.rewrap(scrollback.columns);
            let line_count = wrapped.lines().len();
            scrollback.scroll += line_count;
        }
        scrollback.events.push_front(wrapped);
    }

    pub fn set_suggestion(&self, suggestion: String) {
        let mut input = self.input.lock();
        input.mode = InputMode::Suggesting(suggestion);
    }

    pub fn clear_secure(&self) {
        let mut input = self.input.lock();
        input.mode = InputMode::Text;
        let len = input.buffer.len();
        // Overwrite the input with null bytes
        input.buffer.clear();
        input.buffer.extend(std::iter::repeat('\0').take(len));
        // Reset the buffer.
        input.buffer.clear();
    }

    pub fn set_secure(&self) {
        let mut input = self.input.lock();
        input.mode = InputMode::Secure;
    }

    pub fn clear_input(&self) {
        let mut input = self.input.lock();
        input.buffer.clear();
        if let InputMode::Suggesting(_) = &input.mode {
            input.mode = InputMode::Text;
        }
    }

    pub fn clear_scrollback(&self) {
        let mut scrollback = self.scrollback.lock();
        scrollback.scroll = 0;
        scrollback.events.clear();
    }

    pub fn scroll_to_current(&self) {
        let mut scrollback = self.scrollback.lock();
        scrollback.scroll = 0;
    }
}

#[derive(Default, Clone)]
pub struct Input {
    buffer: Wrapped,
    mode: InputMode,
}

impl Deref for Input {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for Input {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl From<Input> for String {
    fn from(input: Input) -> Self {
        input.buffer.into()
    }
}

pub trait Redrawer: Send + Sync + 'static {
    fn redraw(&mut self);
}

impl<T> Redrawer for T
where
    T: FnMut() + Send + Sync + 'static,
{
    fn redraw(&mut self) {
        self()
    }
}

#[derive(Default, Clone)]
pub enum InputMode {
    #[default]
    Text,
    Suggesting(String),
    Secure,
}
