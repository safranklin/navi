# Pattern Matching in Rust

Pattern matching is one of Rust's most powerful features. The `match` expression allows you to compare a value against a series of patterns and execute code based on which pattern matches.

## Why Pattern Matching?

In many languages, you'd use a series of `if/else` statements or a `switch` statement. Rust's `match` is similar but much more powerful:

1. **Exhaustive checking** — The compiler ensures you handle every possible case
2. **Type safety** — Patterns are checked at compile time
3. **Destructuring** — You can extract values from complex types
4. **No fall-through** — Each arm is independent (unlike C's `switch`)

## Basic Syntax

```rust
match value {
    pattern1 => expression1,
    pattern2 => expression2,
    _ => default_expression,  // The underscore matches anything
}
```

## Real Example from Navi

```rust
match parse_command(&input) {
    Command::Help => {
        println!("Available commands:\n/help - Show this help message\n/quit - Exit the REPL");
    }
    Command::Quit => {
        println!("Exiting navi. Goodbye!");
        break;
    }
    Command::Unknown => {
        println!("Unknown command. Type /help for a list of available commands.");
    }
}
```

### What's happening here?

1. `parse_command(&input)` returns a `Command` enum
2. The `match` checks which variant it is
3. Each arm executes different code
4. The compiler ensures **all** `Command` variants are handled

## Exhaustiveness

This is the magic of Rust's pattern matching. If you forget a case:

```rust
match parse_command(&input) {
    Command::Help => { /* ... */ }
    Command::Quit => { /* ... */ }
    // Forgot Command::Unknown!
}
```

The compiler will error:

```
error[E0004]: non-exhaustive patterns: `Command::Unknown` not covered
```

This prevents bugs! When you add a new `Command` variant later, every `match` in your codebase will fail to compile until you handle the new case.

## The Wildcard Pattern `_`

The `_` pattern matches anything and is often used as a default case:

```rust
fn parse_command(input: &str) -> Command {
    match input.trim() {
        "/quit" => Command::Quit,
        "/help" => Command::Help,
        _ => Command::Unknown,  // Everything else
    }
}
```

## Pattern Matching vs If/Else

You *could* write:

```rust
let cmd = parse_command(&input);
if cmd == Command::Help {
    // ...
} else if cmd == Command::Quit {
    // ...
} else {
    // ...
}
```

But `match` is:
- More concise
- Exhaustively checked by the compiler
- More idiomatic in Rust
- Easier to extend

## Book References

- **Chapter 6.2:** The `match` Control Flow Construct
- **Chapter 18:** Patterns and Matching (advanced patterns)

## Related Concepts

- **Enums** (`concepts/enums.md`) — What you're usually matching against
- **Control Flow** (Chapter 3.5) — Other flow control structures
- **Result/Option** (Chapter 6.1) — Common types to match on
