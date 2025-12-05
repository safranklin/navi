use std::io;
use std::io::Write;

mod api;
use api::{client, ChatMessage};

const MOTD: &str = "Welcome to navi! Type /help for assistance or /quit to exit.";

#[derive(PartialEq)]
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

/// Renders a chat message to the console.
/// 
/// # Arguments
/// * `message` - A reference to the ChatMessage to render.
/// * `add_newline` - A boolean indicating whether to add a newline after the message.
/// 
/// # Returns
/// Nothing. This function prints the message directly to stdout.
/// 
/// # Example
/// ```
/// let message = ChatMessage {
///     role: "navi".to_string(),
///     content: "Hello, user!".to_string(),
/// };
/// render_message(&message, false);
/// // Output:
/// // navi> Hello, user!
/// ```
fn render_message(message: &ChatMessage, add_newline: bool) {
    // Take a chat message, if the role is not user, for now we will assume it's from navi. This should be the case, for now.
    let role_str = match message.role.as_str() {
        "user" => "user",
        _ => "navi",
    };

    let content = &message.content;
    print!("{}> {}", role_str, content);
    if add_newline {
        println!();
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
            role: "user".to_string(),
            content: String::from(""), // Placeholder, will be filled with user input
        };

        render_message(&user_message, false);
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
                render_message(&response_message, true);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }        
    }
}
