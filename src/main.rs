use std::io;
use std::io::Write;

const MOTD: &str = "Welcome to navi! Type /help for assistance or /quit to exit.";

#[derive(PartialEq)]
enum Command {
    Quit,
    Help,
    Unknown,
}

fn parse_command(input: &str) -> Command {
    match input.trim() {
        "/quit" => Command::Quit,
        "/help" => Command::Help,
        _ => Command::Unknown,
    }
}

fn prompt(model_message: &str) -> String {
    let mut input = String::new();
    // Display the model message and prompt the user for input
    println!("navi> {}", model_message);
    print!("user> ");
    io::stdout().flush().expect("Failed to flush stdout!");
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read input!");
    // Return the trimmed user input
    return input.trim().to_string();
}

fn main() {
    println!("{}", MOTD);

    loop {
        let input = prompt("Hey! Listen!");

        match parse_command(&input) {
            Command::Help => {
                println!("Available commands:\n/help - Show this help message\n/quit - Exit the REPL");
            }
            Command::Quit => {
                println!("Exiting navi. Goodbye!");
                break;
            }
            Command::Unknown => { // TODO: Replace with message handling system.`
                println!("Unknown command. Type /help for a list of available commands.");
            }
        }
    }
}
