//! Renders a YouTube-style app icon (red rounded badge + white play triangle)
//! straight into an RGBA buffer, so the window/taskbar icon isn't the generic
//! default exe icon. No image assets, no extra dependencies.

use tao::window::Icon;

pub fn youtube_icon(size: u32) -> Icon {
    let rgba = render(size);
    Icon::from_rgba(rgba, size, size).expect("generated icon buffer is valid RGBA")
}

/// Same artwork, for `tray_icon::Icon` (a distinct type from tao's `Icon`).
pub fn youtube_tray_icon(size: u32) -> tray_icon::Icon {
    let rgba = render(size);
    tray_icon::Icon::from_rgba(rgba, size, size).expect("generated icon buffer is valid RGBA")
}

const SUPERSAMPLE: i32 = 4; // per-axis; 16 samples/pixel for smooth edges

fn render(size: u32) -> Vec<u8> {
    let s = size as f32;
    let margin = s * 0.06;
    let (x0, y0, x1, y1) = (margin, margin + s * 0.10, s - margin, s - margin - s * 0.10);
    let radius = (x1 - x0) * 0.22;

    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let w = x1 - x0;
    let h = y1 - y0;
    let tri = [
        (cx - w * 0.16, cy - h * 0.30),
        (cx - w * 0.16, cy + h * 0.30),
        (cx + w * 0.24, cy),
    ];

    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let mut bg_hits = 0i32;
            let mut tri_hits = 0i32;
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let px = x as f32 + (sx as f32 + 0.5) / SUPERSAMPLE as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SUPERSAMPLE as f32;
                    if in_rounded_rect(px, py, x0, y0, x1, y1, radius) {
                        bg_hits += 1;
                        if in_triangle(px, py, tri[0], tri[1], tri[2]) {
                            tri_hits += 1;
                        }
                    }
                }
            }
            let total = (SUPERSAMPLE * SUPERSAMPLE) as f32;
            let bg_a = bg_hits as f32 / total;
            let tri_a = tri_hits as f32 / total;

            // YouTube red, blended toward white where the triangle covers it.
            let r = lerp(255.0, 255.0, tri_a);
            let g = lerp(0.0, 255.0, tri_a);
            let b = lerp(0.0, 255.0, tri_a);

            let idx = ((y * size + x) * 4) as usize;
            pixels[idx] = r as u8;
            pixels[idx + 1] = g as u8;
            pixels[idx + 2] = b as u8;
            pixels[idx + 3] = (bg_a * 255.0) as u8;
        }
    }
    pixels
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Standard "clamp to inner rect, test distance" rounded-rect containment test.
fn in_rounded_rect(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    if px < x0 || px > x1 || py < y0 || py > y1 {
        return false;
    }
    let cx = px.clamp(x0 + r, x1 - r);
    let cy = py.clamp(y0 + r, y1 - r);
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= r * r
}

fn sign(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    (p.0 - b.0) * (a.1 - b.1) - (a.0 - b.0) * (p.1 - b.1)
}

fn in_triangle(px: f32, py: f32, v1: (f32, f32), v2: (f32, f32), v3: (f32, f32)) -> bool {
    let p = (px, py);
    let d1 = sign(p, v1, v2);
    let d2 = sign(p, v2, v3);
    let d3 = sign(p, v3, v1);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}
