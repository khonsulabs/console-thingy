use parking_lot::Mutex;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::thread::JoinHandle;

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
    console: flume::Sender<ConsoleCommand>,
}

impl Console {
    fn spawn<T: App>(app: T, state: Arc<State>) -> ConsoleHandle {
        let (app_sender, app_receiver) = flume::unbounded();
        let (console_sender, console_receiver) = flume::unbounded();
        let thread = spawn_app(
            app,
            Self {
                state: state.clone(),
                app: app_receiver,
                console: console_sender,
            },
        );
        ConsoleHandle {
            state,
            thread: Some(thread),
            events: Some(app_sender),
            commands: console_receiver,
        }
    }

    pub fn push_line(&self, line: impl Into<String>) {
        self.console
            .send(ConsoleCommand::Push(line.into()))
            .expect("console shut down");
    }

    pub fn set_suggestion(&self, suggestion: impl Into<String>) {
        self.console
            .send(ConsoleCommand::SetSuggestion(suggestion.into()))
            .expect("console shut down");
    }

    pub fn input(&self) -> Input {
        let input = self.state.input.lock();
        input.clone()
    }

    pub fn clear_input(&self) {
        self.console
            .send(ConsoleCommand::ResetInput)
            .expect("console shut down");
    }

    pub fn clear_scrollback(&self) {
        self.console
            .send(ConsoleCommand::ResetScrollback)
            .expect("console shut down");
    }

    pub fn reset_scroll(&self) {
        self.console
            .send(ConsoleCommand::ResetScroll)
            .expect("console shut down");
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
        // If this is the last reference, notify the app the console is shutting
        // down.
        if Arc::strong_count(&self.state) == 1 {
            drop(self.console.send(ConsoleCommand::Shutdown));
        }
    }
}

struct ConsoleHandle {
    state: Arc<State>,
    thread: Option<JoinHandle<anyhow::Result<()>>>,
    events: Option<flume::Sender<ConsoleEvent>>,
    commands: flume::Receiver<ConsoleCommand>,
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
}

pub enum ConsoleEvent {
    InputBufferChanged,
    Input,
}

enum ConsoleCommand {
    Push(String),
    SetSuggestion(String),
    ResetInput,
    ResetScroll,
    ResetScrollback,
    Shutdown,
}

struct State {
    config: Config,
    shutdown: Mutex<bool>,
    input: Mutex<Input>,
}

impl From<Config> for State {
    fn from(config: Config) -> Self {
        Self {
            config,
            shutdown: Mutex::new(false),
            input: Mutex::default(),
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
}

#[derive(Default, Clone)]
pub struct Input {
    buffer: Wrapped,
    suggestion: String,
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
