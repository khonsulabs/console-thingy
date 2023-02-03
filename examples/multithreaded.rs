use std::time::{Duration, Instant};

use console_thingy::{Config, Console};

fn main() {
    Config::default().run(|console: Console| {
        console.push_line(
            "This demo echoes each line of input, and also has events come in from a thread.",
        );
        std::thread::spawn({
            let console = console.clone();
            move || background_message_thread(console)
        });

        while let Ok(event) = console.next_event() {
            if let console_thingy::ConsoleEvent::Input = event {
                let input = String::from(console.input());
                match &*input {
                    "exit" | "quit" => {
                        break;
                    }
                    // TODO these don't shut down the app correctly, but it
                    // seems to be because Kludgine doesn't notice the
                    // threads are shut down:
                    // https://github.com/khonsulabs/kludgine/issues/59
                    "error" => {
                        anyhow::bail!("an error occurred");
                    }
                    "panic" => {
                        panic!("this is a panic");
                    }
                    _ => {
                        console.push_line(input);
                        console.clear_input();
                        console.reset_scroll();
                    }
                }
            }
        }

        Ok(())
    })
}

fn background_message_thread(console: Console) {
    let start = Instant::now();
    while !console.should_shutdown() {
        std::thread::sleep(Duration::from_secs(1));

        let words = (start.elapsed().subsec_nanos() % 31) as usize;
        console.push_line(lipsum::lipsum_words(words));
    }
}
