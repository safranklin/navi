# The REPL Pattern

**Book Reference:** Chapter 2 — Programming a Guessing Game

## What is a REPL?

**REPL** stands for **Read-Eval-Print-Loop**:
1. **Read** — Get input from the user
2. **Eval** — Process/evaluate that input
3. **Print** — Display the result
4. **Loop** — Repeat

This pattern is fundamental to interactive programs:
- Shells (bash, zsh, fish)
- Language interpreters (Python, Node.js, irb)
- Database clients (psql, mysql)
- Interactive tools (debuggers, calculators)

## Basic Implementation in Rust

```rust
use std::io::{self, Write};

fn main() {
    loop {
        // Print prompt
        print!("navi> ");
        io::stdout().flush().expect("Failed to flush stdout");

        // Read input
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");

        // Eval (for now, just echo)
        let input = input.trim();

        // Print result
        println!("You said: {}", input);

        // Loop continues...
    }
}
```

## Key Rust Concepts

### 1. The `loop` Keyword
Creates an infinite loop. You can break out with:
- `break` — exit the loop
- `return` — exit the entire function
- Or check a condition like `if input == "exit" { break; }`

### 2. Reading Input: `stdin().read_line()`
```rust
let mut input = String::new();
io::stdin()
    .read_line(&mut input)
    .expect("Failed to read line");
```

**Key points:**
- Must create a mutable `String` to store input
- Pass a mutable reference (`&mut input`) to `read_line()`
- Returns a `Result` — use `.expect()` for simple error handling
- Includes the newline character — use `.trim()` to remove it

### 3. Flushing stdout
```rust
print!("navi> ");
io::stdout().flush().expect("Failed to flush");
```

**Why flush?**
- `print!()` (without `ln`) doesn't automatically flush
- Without flushing, the prompt might not appear until after input
- `println!()` auto-flushes, but we want the prompt on the same line as input

### 4. Mutable References (`&mut`)
Rust's ownership system requires explicit mutability:
- `let input = String::new()` — immutable by default
- `let mut input = String::new()` — explicitly mutable
- `&mut input` — a mutable reference, allows the function to modify our string

This is covered in **Chapter 4 — Understanding Ownership**.

## Evolution Path

This basic REPL can evolve:

### Phase 1: Basic Loop (Done!)
- Display prompt
- Read input
- Echo back
- Repeat

### Phase 2: Command Parsing
```rust
match input.trim() {
    "exit" | "quit" => break,
    "help" => println!("Available commands: help, exit"),
    _ => println!("You said: {}", input),
}
```

### Phase 3: Multi-line Input
For code snippets or longer messages, allow entering multiple lines.

### Phase 4: AI Integration
Send input to an AI provider, stream the response back.

### Phase 5: Context Management
Keep conversation history, manage tokens, etc.

## Common Patterns

### Error Handling
Instead of `.expect()`, use `match` or `?`:
```rust
match io::stdin().read_line(&mut input) {
    Ok(_) => { /* process input */ }
    Err(e) => {
        eprintln!("Error reading input: {}", e);
        continue;
    }
}
```

### Trimming Input
Always trim to remove newlines and whitespace:
```rust
let input = input.trim();
```

### Exit Conditions
```rust
if input == "exit" || input == "quit" {
    println!("Goodbye!");
    break;
}
```

## Real-World Examples

### Python's REPL
```
>>> 2 + 2
4
>>> print("hello")
hello
>>> exit()
```

### Bash Shell
```
$ echo "hello"
hello
$ ls
file1.txt  file2.txt
$ exit
```

### Navi (our implementation)
```
navi> What is Rust?
[AI response would go here]
navi> exit
Goodbye!
```

## Key Takeaway

The REPL pattern is simple but powerful. It's the foundation for building interactive tools. Start with a basic loop, then layer on features like command parsing, error handling, and eventually AI integration.
