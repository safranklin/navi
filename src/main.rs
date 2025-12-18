use std::io;
use std::io::Write;

mod api;
use api::{client, ChatMessage, Role};

const MOTD: &str = "Welcome to navi! Type /help for assistance or /quit to exit.";

#[derive(PartialEq, Debug)]
enum Command {
    Quit,
    Help,
    Unknown,
}

/// Trys to parse a command from user input. If the input does not match any known commands, it returns Command::Unknown, which is expected in most cases.
/// 
/// # Arguments
/// * `input` - A string slice containing the user input.
/// 
/// # Returns
/// A Command enum variant corresponding to the input.
/// 
/// # Example
/// ```
/// let command = parse_command("/help");
/// assert_eq!(command, Command::Help);
/// ```
/// # Notes
/// This function currently recognizes only /quit and /help commands. Any other input is classified as Unknown.
fn parse_command(input: &str) -> Command {
    match input.trim() {
        "/quit" => Command::Quit,
        "/help" => Command::Help,
        _ => Command::Unknown,
    }
}

/// Captures user input from stdin.
/// 
/// # Returns
/// A String containing the user's input.
/// 
/// # Example
/// ```
/// let user_input = capture_user_input();
/// println!("You entered: {}", user_input);
/// // Output:
/// // (waits for user input)
/// // You entered: <user input>
/// ```
/// # Panics
/// This function will panic if it fails to read from stdin.
///     
fn capture_user_input() -> String {
    let mut input = String::new();
    io::stdout().flush().expect("Failed to flush stdout!");
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read input!");
    input.trim().to_string()
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();  // Load .env file
    println!("{}\n", MOTD);

    loop {

        let mut user_message = ChatMessage {
            role: Role::User,
            content: String::from(""), // Placeholder, will be filled with user input
        };

        print!("{}", user_message);
        user_message.content = capture_user_input();

        // Check for commands
        match parse_command(&user_message.content) {
            Command::Quit => {
                println!("Exiting navi. Goodbye!");
                break;
            }
            Command::Help => {
                println!("Available commands:\n/quit - Exit the application\n/help - Show this help message\n");
                continue;
            }
            _ => {}
        }

        match client::chat_completion(&user_message).await {
            Ok(response_message) => {
                println!("{}", response_message);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }        
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command_quit() {
        assert_eq!(parse_command("/quit"), Command::Quit);
    }

    #[test]
    fn test_parse_command_help() {
        assert_eq!(parse_command("/help"), Command::Help);
    }

    #[test]
    fn test_parse_normal_input() {
        assert_eq!(parse_command("Hello, how are you?"), Command::Unknown);
    }
}