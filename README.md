# console-thingy

This is an experiment born out of wanting a basic framework for building apps
that have a basic input prompt and an interactive stream of events. It is
currently only a proof-of-concept.

## Goals

- Runs in a variety of modes
  - [x] "Native" GUI app
    - Uses [Kludgine][kludgine], which is wgpu-based.
  - Embeddable in an exiting Kludgine app
    - Similar to how many games pop up a console overlay, this mode would allow
      Kludgine-based apps to render the console view directly in their Scene.
  - TUI app
    - The needed functionality is so basic, I plan to just use crossterm directly.
- Easy to use
- A few basic input modes:
  - Text, with enough events to implement type-ahead suggestions
  - Multi-choice
- Ability to pin one or more messages, allowing important messages to remain
  on-screen while other events continue scrolling.

## Example

This example echoes all input to the output console:

```rust
use console_thingy::{Config, Console};

fn main() {
    Config::default().run(|console: Console| {
        console.push_line("This demo echoes each line of input.");

        while let Ok(event) = console.next_event() {
            match event {
                console_thingy::ConsoleEvent::Input => {
                    console.push_line(console.input());
                    console.clear_input();
                    console.reset_scroll();
                }
            }
        }

        Ok(())
    })
}
```

This is the above example running as a GUI application:

<video src="https://raw.githubusercontent.com/khonsulabs/console-thingy/gh-pages/echo-screencast.mp4"></video>

[kludgine]: https://github.com/khonsulabs/kludgine
