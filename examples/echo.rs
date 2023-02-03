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
