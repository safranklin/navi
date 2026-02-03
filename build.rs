use std::env;
use std::fs;
use std::path::Path;
use std::io::Write;

fn main() {
    println!("cargo:rerun-if-changed=assets/");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("landing_frames.rs");
    let mut f = fs::File::create(&dest_path).unwrap();

    let asset_dir = Path::new("assets");
    if !asset_dir.exists() {
        // If no assets, just write an empty array to avoid build failure
        writeln!(f, "const ASCII_FRAMES: &[&str] = &[];").unwrap();
        return;
    }

    let mut entries: Vec<_> = fs::read_dir(asset_dir)
        .unwrap()
        .map(|res| res.unwrap().path())
        .filter(|path| path.extension().map_or(false, |ext| ext == "txt"))
        .collect();

    // Sort to ensure frame order
    entries.sort();

    // Pass 1: Find the Global Vertical Padding (Top/Bottom)
    // We want to find the *minimum* number of empty lines at the top and bottom shared by ALL frames.
    // We will strip this "global padding" from every frame.
    // This preserves the relative vertical animation (jumping up and down) while removing the static letterboxing.

    let is_blank = |line: &str| line.chars().all(|c| c == ' ' || c == '⠀');
    
    let mut global_top_padding = usize::MAX;
    let mut global_bottom_padding = usize::MAX;

    for path in &entries {
        let content = fs::read_to_string(path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        
        // Find top padding for this frame
        let mut top = 0;
        for line in &lines {
            if is_blank(line) { top += 1; } else { break; }
        }
        
        // Find bottom padding for this frame
        let mut bottom = 0;
        for line in lines.iter().rev() {
            if is_blank(line) { bottom += 1; } else { break; }
        }
        
        // Update global minimums
        if top < global_top_padding { global_top_padding = top; }
        if bottom < global_bottom_padding { global_bottom_padding = bottom; }
    }
    
    // Pass 1.5: Find Global Max Width of content (to ensure consistent centering)
    // If lines have different lengths, Ratatui's Alignment::Center will shift them relative to each other,
    // destroying the internal alignment of the art.
    // We must pad all lines to the same width.
    
    let mut global_max_width = 0;
    
    for path in &entries {
        let content = fs::read_to_string(path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        
        let start = global_top_padding;
        let end = lines.len().saturating_sub(global_bottom_padding);
        
        if start < end && start < lines.len() {
            for line in &lines[start..end] {
                // We use trim_end matches for calculation to see "intended" width
                // including leading spaces but ignoring right-side padding
                let trimmed = line.trim_end_matches(|c| c == ' ' || c == '⠀');
                let width = trimmed.chars().count();
                if width > global_max_width { global_max_width = width; }
            }
        }
    }

    writeln!(f, "const ASCII_FRAMES: &[&str] = &[").unwrap();

    for path in entries {
        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut processed_lines = Vec::new();

        let start = global_top_padding;
        let end = lines.len().saturating_sub(global_bottom_padding);
        
        if start < end && start < lines.len() {
            for line in &lines[start..end] {
                let trimmed = line.trim_end_matches(|c| c == ' ' || c == '⠀');
                let mut padded = trimmed.to_string();
                
                // Pad to global_max_width to preserve alignment block
                while padded.chars().count() < global_max_width {
                    padded.push(' ');
                }
                
                processed_lines.push(padded);
            }
        }

        // Escape for Rust string literal
        let mut final_parts = Vec::new();
        for line in processed_lines {
            let escaped = line.replace('\\', "\\\\").replace('"', "\\\"");
            final_parts.push(escaped);
        }
        let final_str = final_parts.join("\\n");
        
        writeln!(f, "    \"{}\",", final_str).unwrap();
    }

    writeln!(f, "];").unwrap();
}
