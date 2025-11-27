# Enums in Rust

Enums (short for "enumerations") allow you to define a type by enumerating its possible values. They're one of Rust's most powerful features for modeling your domain.

## Why Enums?

Instead of using magic strings or numbers to represent states, you create a type-safe representation of "one of several options."

**Without enums (error-prone):**
```rust
let command_type = "quit";  // Easy to typo, hard to catch
if command_type == "quite" {  // Typo! But compiles fine
    // Never runs
}
```

**With enums (type-safe):**
```rust
enum Command {
    Quit,
    Help,
}

let cmd = Command::Quit;
// Compiler ensures you can only use valid variants
```

## Basic Syntax

```rust
enum Command {
    Quit,
    Help,
    Unknown,
}
```

Each variant (`Quit`, `Help`, `Unknown`) is a possible value of type `Command`.

## Creating Enum Values

Use the `::` syntax:

```rust
let quit_cmd = Command::Quit;
let help_cmd = Command::Help;
```

## Using Enums with Pattern Matching

Enums and `match` work together beautifully:

```rust
fn parse_command(input: &str) -> Command {
    match input.trim() {
        "/quit" => Command::Quit,
        "/help" => Command::Help,
        _ => Command::Unknown,
    }
}

fn handle_command(cmd: Command) {
    match cmd {
        Command::Quit => println!("Goodbye!"),
        Command::Help => println!("Help info..."),
        Command::Unknown => println!("Unknown command"),
    }
}
```

The compiler ensures you handle **every** variant. Add a new variant? Every `match` will fail to compile until you handle it.

## Deriving Traits

By default, you can't compare or print enum values. You need to explicitly opt in:

```rust
#[derive(PartialEq, Debug)]
enum Command {
    Quit,
    Help,
}
```

- `PartialEq` — Allows `==` and `!=` comparisons
- `Debug` — Allows printing with `{:?}` for debugging

### Common Derivable Traits

```rust
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Command {
    Quit,
    Help,
}
```

- **Debug** — Print for debugging (`println!("{:?}", cmd)`)
- **PartialEq** — Equality comparison (`cmd == Command::Quit`)
- **Eq** — Full equality (stricter than PartialEq)
- **Clone** — Explicit cloning (`cmd.clone()`)
- **Copy** — Implicit copying (for simple types)

## Enums Can Hold Data

Enums in Rust are more powerful than in most languages — variants can hold data:

```rust
enum Command {
    Quit,
    Help,
    Say(String),  // This variant holds a String
    Move { x: i32, y: i32 },  // This variant holds named fields
}

let cmd = Command::Say("Hello!".to_string());

match cmd {
    Command::Say(message) => println!("Saying: {}", message),
    Command::Move { x, y } => println!("Moving to ({}, {})", x, y),
    // ... other variants
}
```

This is incredibly powerful! You can model complex domain logic.

## Enums vs Structs

- **Struct** — A type that groups related data together
- **Enum** — A type that represents "one of several options"

```rust
struct User {
    name: String,
    age: u32,
}  // A user HAS a name AND age

enum Message {
    Quit,
    Text(String),
    Move { x: i32, y: i32 },
}  // A message is EITHER Quit OR Text OR Move
```

## Real Example from Navi

```rust
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
```

This gives us:
1. **Type safety** — Can't accidentally use an invalid command
2. **Exhaustive handling** — Compiler ensures we handle all commands
3. **Clear intent** — The code documents itself
4. **Easy extension** — Add new commands by adding variants

## Common Standard Library Enums

### Option<T>

Represents something that might or might not exist:

```rust
enum Option<T> {
    Some(T),
    None,
}

let maybe_number: Option<i32> = Some(5);
let no_number: Option<i32> = None;
```

### Result<T, E>

Represents success or failure:

```rust
enum Result<T, E> {
    Ok(T),
    Err(E),
}

let success: Result<i32, String> = Ok(42);
let failure: Result<i32, String> = Err("Error message".to_string());
```

You've already been using `Result`! When you call `.expect()` on I/O operations, you're handling a `Result`.

## Book References

- **Chapter 6:** Enums and Pattern Matching
- **Chapter 6.1:** Defining an Enum
- **Chapter 6.2:** The `match` Control Flow Construct
- **Chapter 6.3:** Concise Control Flow with `if let`
- **Chapter 5.2:** Example Program Using Structs (introduces `derive`)

## Related Concepts

- **Pattern Matching** (`concepts/pattern-matching.md`) — How to use enums effectively
- **Traits** (`concepts/traits.md`) — What you derive on enums
- **Option and Result** (Chapter 6.1, 9.2) — Essential standard library enums
