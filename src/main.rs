use std::io;
use std::io::Write;

fn main() {
    let mut input = String::new();
    let welcome_message = "Welcome to Navi! Type 'quit' to exit.";

    while input.trim() != "quit" {
        println!("\nnavi> {welcome_message}");

        input.clear(); // Clear previous input if any

        print!("user> ");
        io::stdout().flush().expect("Failed to flush stdout!");

        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read input!");
    }
}
