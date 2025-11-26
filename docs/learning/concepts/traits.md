# Traits and Scoping

**Book Reference:** Chapter 10.2 — Traits: Defining Shared Behavior

## The Core Concept

In Rust, **traits must be in scope to use their methods**, even when a type already implements that trait.

### Why This Rule Exists

This design prevents:
1. **Name conflicts** — Multiple traits can define methods with the same name
2. **Unclear origins** — Explicit imports make it obvious where methods come from
3. **Namespace pollution** — You only get the methods you explicitly ask for

## Real Example from Navi

### The Error
```
error[E0599]: no method named `flush` found for struct `Stdout`
 --> src/main.rs:8:22
  |
8 |         io::stdout().flush().expect("Failed to flush stdout!");
  |                      ^^^^^ method not found in `Stdout`
```

### The Problem
- `io::stdout()` returns a `Stdout` struct
- `Stdout` implements the `Write` trait
- The `flush()` method is defined in `Write`, not directly on `Stdout`
- We imported `std::io` but not `std::io::Write`

### The Fix
```rust
use std::io::Write;  // Now flush() is available
```

## Understanding Trait Methods vs Direct Methods

When you see a method call like `my_value.some_method()`:
- It could be a method directly defined on the type
- OR it could be from a trait that the type implements

**The rule:** Trait methods require the trait to be in scope. Direct methods don't.

## Comparison with Other Languages

### C# — Extension Method Conflicts
```csharp
using Library1;  // Has string.Validate()
using Library2;  // Also has string.Validate()

myString.Validate();  // ERROR: Ambiguous!
```
Once imported, extension methods are always available, leading to conflicts.

### Python — Monkey Patching
```python
# Library A
str.custom = lambda self: "A"

# Library B (imported later)
str.custom = lambda self: "B"  # Silently overwrites!

"hello".custom()  # Which one? Depends on import order
```
Libraries can modify types globally, causing unpredictable behavior.

### JavaScript — Prototype Pollution
```javascript
String.prototype.sanitize = function() { /* ... */ }  // Library A
String.prototype.sanitize = function() { /* ... */ }  // Library B overwrites

"test".sanitize()  // Last one wins
```

### Rust — Explicit Choice
```rust
use library1::Parse;  // Only this trait's methods are available
use library2::Format; // Different trait, no conflict

my_string.parse();   // Unambiguous — comes from library1::Parse
my_string.format();  // Unambiguous — comes from library2::Format
```

If both traits have the same method name, you can disambiguate:
```rust
use library1::Parse as Parse1;
use library2::Parse as Parse2;

Parse1::parse(&my_string);  // Explicit
Parse2::parse(&my_string);  // No ambiguity
```

## Common Traits You'll Import

- `std::io::Read` — for `.read()`, `.read_to_string()`, etc.
- `std::io::Write` — for `.write()`, `.flush()`, etc.
- `std::fmt::Display` — for custom formatting with `println!`
- `std::str::FromStr` — for `.parse()` on strings
- `std::cmp::PartialOrd` — for comparison operators

## Key Takeaway

Rust's trait scoping is **explicit by design**. You always know where a method comes from, and multiple libraries can safely extend the same type without conflicts. When you see a "method not found" error, check if you need to import a trait!
