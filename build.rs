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
        writeln!(f, "const ASCII_FRAMES: &[&str] = &[];").unwrap();
        return;
    }

    let mut entries: Vec<_> = fs::read_dir(asset_dir)
        .unwrap()
        .map(|res| res.unwrap().path())
        .filter(|path| path.extension().map_or(false, |ext| ext == "txt"))
        .collect();

    entries.sort();

    // Braille Dot Map (ISO 11548-1)
    // 1 4
    // 2 5
    // 3 6
    // 7 8
    fn get_dots(c: char) -> Vec<(f64, f64)> {
        let code = c as u32;
        let mut points = Vec::new();
        if !(0x2800..=0x28FF).contains(&code) { return points; }
        
        let pattern = code - 0x2800;
        
        // (dx, dy)
        let offsets = [
            (0x01, 0.0, 0.0), // Dot 1
            (0x02, 0.0, 1.0), // Dot 2
            (0x04, 0.0, 2.0), // Dot 3
            (0x08, 1.0, 0.0), // Dot 4
            (0x10, 1.0, 1.0), // Dot 5
            (0x20, 1.0, 2.0), // Dot 6
            (0x40, 0.0, 3.0), // Dot 7
            (0x80, 1.0, 3.0), // Dot 8
        ];

        for (mask, dx, dy) in offsets {
            if pattern & mask != 0 {
                points.push((dx, dy));
            }
        }
        points
    }

    writeln!(f, "pub const LOGO_FRAMES: &[&[(f64, f64)]] = &[").unwrap();
    
    let mut global_min_x = f64::MAX;
    let mut global_max_x = f64::MIN;
    let mut global_min_y = f64::MAX;
    let mut global_max_y = f64::MIN;

    // We process paths to strings first to hold data
    struct FrameData {
        points: Vec<(f64, f64)>,
    }
    let mut frames_data = Vec::new();

    for path in entries {
        let content = fs::read_to_string(&path).unwrap();
        let mut points = Vec::new();
        
        for (row, line) in content.lines().enumerate() {
            // Remove BOM or weird chars if any (though Rust strings are UTF-8)
            // The file might contain spaces for padding. 
            // Spaces are NOT braille, so they add partial width but no dots.
            // But we operate on Grid.
            
            for (col, c) in line.chars().enumerate() {
               let dot_offsets = get_dots(c);
               for (dx, dy) in dot_offsets {
                   let x = (col as f64) * 2.0 + dx;
                   let y = (row as f64) * 4.0 + dy;
                   // Flip Y? Canvas usually has Y going up?
                   // No, Ratatui Canvas coordinates: (0,0) is usually bottom-left?
                   // Depends on x_bounds / y_bounds.
                   // Let's assume standard image coords (y down) and flip in render or setting bounds.
                   // Actually, BrailleCanvas usually draws mathematically (Y up).
                   // Let's just store "Image Coords" (Y Down) here: 0 is top.
                   // When rendering, we can map Top (0) to Top of widget.
                   
                   points.push((x, y));
                   
                   if x < global_min_x { global_min_x = x; }
                   if x > global_max_x { global_max_x = x; }
                   if y < global_min_y { global_min_y = y; }
                   if y > global_max_y { global_max_y = y; }
               }
            }
        }
        frames_data.push(FrameData { points });
    }
    
    let mut total_x_sum: f64 = 0.0;
    let mut total_points_count: usize = 0;

    for frame in &frames_data {
        write!(f, "    &[").unwrap();
        for (x, y) in &frame.points {
             // Normalized coordinates
             let norm_x = x - global_min_x;
             let norm_y = y - global_min_y;
             
             write!(f, "({:.1}, {:.1}), ", norm_x, norm_y).unwrap();
             
             total_x_sum += norm_x;
             total_points_count += 1;
        }
        writeln!(f, "],").unwrap();
    }
    
    writeln!(f, "];").unwrap();
    
    let width = global_max_x - global_min_x;
    let height = global_max_y - global_min_y;
    
    let center_of_mass_x = if total_points_count > 0 {
        total_x_sum / (total_points_count as f64)
    } else {
        width / 2.0
    };
    
    writeln!(f, "pub const LOGO_WIDTH: f64 = {:.1};", width).unwrap();
    writeln!(f, "pub const LOGO_HEIGHT: f64 = {:.1};", height).unwrap();
    writeln!(f, "pub const LOGO_COM_X: f64 = {:.2};", center_of_mass_x).unwrap();

}
