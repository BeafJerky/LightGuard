use std::sync::atomic::{AtomicBool, Ordering};

use tauri::image::Image;
use tauri::{AppHandle, Manager};

const BADGE_RGBA: [u8; 4] = [0xE1, 0x3B, 0x3B, 0xFF];
const LABEL_RGBA: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

const TRAY_ID: &str = "lightguard-tray";

static NOTIFICATION_DENIED: AtomicBool = AtomicBool::new(false);

pub fn notification_denied() -> bool {
    NOTIFICATION_DENIED.load(Ordering::Relaxed)
}

pub fn badge_label(count: u32) -> String {
    match count {
        0 => String::new(),
        1..=99 => count.to_string(),
        _ => "99+".to_string(),
    }
}

const GLYPH_W: u32 = 3;
const GLYPH_H: u32 = 5;

const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

fn glyph(ch: char) -> Option<[u8; 5]> {
    match ch {
        '0'..='9' => Some(DIGITS[(ch as u8 - b'0') as usize]),
        '+' => Some([0b000, 0b010, 0b111, 0b010, 0b000]),
        _ => None,
    }
}

fn set_pixel(rgba: &mut [u8], width: u32, height: u32, x: u32, y: u32, color: [u8; 4]) {
    if x >= width || y >= height {
        return;
    }
    let Some(offset) = (y as usize)
        .checked_mul(width as usize)
        .and_then(|row| row.checked_add(x as usize))
        .and_then(|index| index.checked_mul(4))
    else {
        return;
    };
    let Some(end) = offset.checked_add(4) else {
        return;
    };
    let Some(pixel) = rgba.get_mut(offset..end) else {
        return;
    };
    pixel.copy_from_slice(&color);
}

#[derive(Clone, Copy)]
struct BadgeGeometry {
    start_x: u32,
    start_y: u32,
    diameter: u32,
}

fn badge_geometry(width: u32, height: u32) -> Option<BadgeGeometry> {
    if width == 0 || height == 0 {
        return None;
    }
    let min_side = width.min(height);
    let diameter = (min_side * 5 / 8).max(3).min(min_side);
    Some(BadgeGeometry {
        start_x: width - diameter,
        start_y: height - diameter,
        diameter,
    })
}

/// 角标贴住右下角：只有内侧（左上）圆角，外侧直角覆盖到角落像素。
fn badge_mask(width: u32, height: u32) -> Vec<bool> {
    let Some(badge) = badge_geometry(width, height) else {
        return Vec::new();
    };
    let Some(total) = (width as usize).checked_mul(height as usize) else {
        return Vec::new();
    };
    let mut mask = vec![false; total];
    let radius = (badge.diameter as f64 / 4.0).max(1.0);
    let arc_x = badge.start_x as f64 + radius;
    let arc_y = badge.start_y as f64 + radius;
    for y in badge.start_y..badge.start_y + badge.diameter {
        for x in badge.start_x..badge.start_x + badge.diameter {
            if (x as f64) < arc_x && (y as f64) < arc_y {
                let dx = x as f64 - arc_x;
                let dy = y as f64 - arc_y;
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
            }
            mask[y as usize * width as usize + x as usize] = true;
        }
    }
    mask
}

/// 字形整体在小角标内按两轴等比缩放，四周至少留 1 px 内边距。
fn glyph_scale(glyph_count: u32, diameter: u32) -> (u32, u32, u32) {
    let spacing = 1;
    let run_w = glyph_count * GLYPH_W + glyph_count.saturating_sub(1) * spacing;
    let available = diameter.saturating_sub(2);
    let width_scale = available / run_w.max(1);
    let height_scale = available / GLYPH_H;
    let scale = width_scale.min(height_scale).max(1);
    (scale, run_w * scale, GLYPH_H * scale)
}

pub fn draw_badge(base: &[u8], width: u32, height: u32, count: u32) -> Vec<u8> {
    let mut rgba = base.to_vec();
    let label = badge_label(count);
    let Some(badge) = badge_geometry(width, height) else {
        return rgba;
    };
    if label.is_empty() {
        return rgba;
    }

    // 先算掩码，红色底与白色字形都只落在掩码像素上，圆角外不会漏色。
    let mask = badge_mask(width, height);
    let is_painted = |x: u32, y: u32| -> bool {
        if x >= width || y >= height {
            return false;
        }
        mask.get(y as usize * width as usize + x as usize)
            .copied()
            .unwrap_or(false)
    };
    for y in badge.start_y..badge.start_y + badge.diameter {
        for x in badge.start_x..badge.start_x + badge.diameter {
            if is_painted(x, y) {
                set_pixel(&mut rgba, width, height, x, y, BADGE_RGBA);
            }
        }
    }

    let glyphs: Vec<[u8; 5]> = label.chars().filter_map(glyph).collect();
    if glyphs.is_empty() {
        return rgba;
    }
    let glyph_count = glyphs.len() as u32;
    let (scale, total_w, total_h) = glyph_scale(glyph_count, badge.diameter);
    let origin_x = badge.start_x + badge.diameter.saturating_sub(total_w) / 2;
    let origin_y = badge.start_y + badge.diameter.saturating_sub(total_h) / 2;
    for (index, rows) in glyphs.iter().enumerate() {
        let glyph_x = origin_x + index as u32 * (GLYPH_W + 1) * scale;
        for (row, bits) in rows.iter().enumerate() {
            for column in 0..GLYPH_W {
                if (bits >> (GLYPH_W - 1 - column)) & 1 == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = glyph_x + column * scale + dx;
                        let py = origin_y + row as u32 * scale + dy;
                        if !is_painted(px, py) {
                            continue;
                        }
                        set_pixel(&mut rgba, width, height, px, py, LABEL_RGBA);
                    }
                }
            }
        }
    }
    rgba
}

pub fn refresh_badge(app: &AppHandle, count: u32) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let tooltip = if count == 0 {
        "LightGuard".to_string()
    } else {
        format!("LightGuard · {count} 条未处理告警")
    };
    let _ = tray.set_tooltip(Some(tooltip));
    let Some(base) = app.default_window_icon() else {
        return;
    };
    let image = Image::new_owned(
        draw_badge(base.rgba(), base.width(), base.height(), count),
        base.width(),
        base.height(),
    );
    let _ = tray.set_icon(Some(image.clone()));
    #[cfg(target_os = "windows")]
    if let Some(window) = app.get_webview_window("main") {
        let overlay = if count == 0 { None } else { Some(image) };
        let _ = window.set_overlay_icon(overlay);
    }
}

fn severity_label(severity: &str) -> &'static str {
    match severity {
        "critical" => "严重",
        "warning" => "警告",
        _ => "提示",
    }
}

pub fn notify_alert(app: &AppHandle, severity: &str, server_name: &str, headline: &str) -> bool {
    use tauri_plugin_notification::NotificationExt;

    if NOTIFICATION_DENIED.load(Ordering::Relaxed) {
        return false;
    }
    let title = format!("{} · {} · {}", severity_label(severity), server_name, headline);
    match app
        .notification()
        .builder()
        .title(title)
        .body("打开 LightGuard 查看告警中心")
        .show()
    {
        Ok(()) => true,
        Err(error) => {
            let text = error.to_string().to_lowercase();
            if text.contains("permission") || text.contains("denied") {
                NOTIFICATION_DENIED.store(true, Ordering::Relaxed);
            }
            false
        }
    }
}

#[tauri::command]
pub fn notify_test(app: AppHandle) -> Result<bool, String> {
    Ok(notify_alert(&app, "info", "LightGuard", "测试通知"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_hides_zero_and_caps_at_99_plus() {
        assert_eq!(badge_label(0), "");
        assert_eq!(badge_label(1), "1");
        assert_eq!(badge_label(99), "99");
        assert_eq!(badge_label(100), "99+");
        assert_eq!(badge_label(5_000), "99+");
    }

    #[test]
    fn zero_count_keeps_base_pixels() {
        let base = vec![9u8; 32 * 32 * 4];
        assert_eq!(draw_badge(&base, 32, 32, 0), base);
    }

    #[test]
    fn badge_paints_bottom_right_corner_red() {
        let base = vec![0u8; 32 * 32 * 4];
        let painted = draw_badge(&base, 32, 32, 7);
        assert_eq!(painted.len(), base.len());
        let corner = (31 * 32 + 31) * 4;
        assert!(painted[corner] > 200, "red channel at corner");
        assert!(painted[corner + 1] < 120, "green channel at corner");
        assert_eq!(painted[corner + 3], 0xFF, "corner is opaque");
    }

    #[test]
    fn badge_adds_white_label_pixels() {
        let base = vec![0u8; 32 * 32 * 4];
        let painted = draw_badge(&base, 32, 32, 12);
        let white = painted
            .chunks_exact(4)
            .filter(|pixel| pixel[0] == 0xFF && pixel[1] == 0xFF && pixel[2] == 0xFF)
            .count();
        assert!(white >= 5, "expected white glyph pixels, got {white}");
    }

    fn count_pixels(rgba: &[u8], color: [u8; 4]) -> usize {
        rgba.chunks_exact(4).filter(|pixel| *pixel == color).count()
    }

    #[test]
    fn severity_labels_are_localized() {
        assert_eq!(severity_label("critical"), "严重");
        assert_eq!(severity_label("warning"), "警告");
        assert_eq!(severity_label("info"), "提示");
        assert_eq!(severity_label("unknown"), "提示");
        for raw in ["critical", "warning", "info", "unknown"] {
            let label = severity_label(raw);
            assert_ne!(label, raw, "raw severity leaked into the label");
            assert!(
                label.chars().all(|c| !c.is_ascii_alphabetic()),
                "label {label:?} contains English letters"
            );
        }
    }

    #[test]
    fn white_glyphs_stay_inside_badge_mask() {
        let width = 32;
        let height = 32;
        let mask = badge_mask(width, height);
        assert!(
            mask.iter().filter(|painted| **painted).count() > 0,
            "badge mask is empty"
        );
        for count in [7u32, 12, 100] {
            let base = vec![0u8; (width * height * 4) as usize];
            let painted = draw_badge(&base, width, height, count);
            assert!(
                count_pixels(&painted, BADGE_RGBA) > 0,
                "no badge red painted for {count}"
            );
            let white_pixels = painted.chunks_exact(4).enumerate().filter(|(_, pixel)| {
                pixel[0] == 0xFF && pixel[1] == 0xFF && pixel[2] == 0xFF
            });
            let mut whites = 0usize;
            for (index, _) in white_pixels {
                whites += 1;
                assert!(
                    mask.get(index).copied().unwrap_or(false),
                    "white glyph pixel at ({}, {}) escapes the badge mask for {count}",
                    index % width as usize,
                    index / width as usize
                );
            }
            assert!(whites > 0, "expected white glyph pixels for {count}");
        }
    }

    #[test]
    fn small_icon_still_paints_badge() {
        let base = vec![0u8; 16 * 16 * 4];
        let painted = draw_badge(&base, 16, 16, 100);
        assert_eq!(painted.len(), base.len());
        assert!(
            count_pixels(&painted, BADGE_RGBA) > 0,
            "16x16 icon should still show a badge"
        );
    }

    #[test]
    fn short_buffer_does_not_panic() {
        let base = vec![0u8; 8];
        let painted = draw_badge(&base, 32, 32, 7);
        assert_eq!(painted.len(), base.len());
    }
}
