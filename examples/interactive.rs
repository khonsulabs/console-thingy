use console_thingy::{Config, Console, ConsoleEvent};

fn main() {
    Config::default().run(|console: Console| {
        console.push_line(
            "This demo echoes each line of input and has a few slash commands (e.g., /exit)",
        );

        while let Ok(event) = console.next_event() {
            let input = console.input();
            match event {
                ConsoleEvent::InputBufferChanged => {
                    if let Some(command) = input.strip_prefix('/') {
                        if "quit".starts_with(command) {
                            console.set_suggestion(&"quit"[command.len()..]);
                        } else if "exit".starts_with(command) {
                            console.set_suggestion(&"exit"[command.len()..]);
                        } else if "clear".starts_with(command) {
                            console.set_suggestion(&"clear"[command.len()..]);
                        } else {
                            console.set_suggestion("");
                        }
                    }
                }
                ConsoleEvent::Input => {
                    if let Some(command) = input.strip_prefix('/') {
                        match command {
                            "e" | "q" | "exit" | "quit" => break,
                            "clear" => console.clear_scrollback(),
                            _ => {
                                console.push_line(format!("unknown command /{command}"));
                            }
                        }
                    } else {
                        console.push_line(input);
                    }
                    console.clear_input();
                    console.reset_scroll();
                }
            }
        }

        Ok(())
    })
}
