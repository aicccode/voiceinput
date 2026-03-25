use cpal::traits::{DeviceTrait, HostTrait};
use tauri::{
    image::Image,
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    tray::TrayIconBuilder,
    AppHandle,
};

const ICON_SIZE: u32 = 32;

/// Build and register the system tray with full menu
pub fn setup_tray(handle: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let menu = build_tray_menu(handle)?;
    let icon = generate_icon(TrayState::Idle);

    let _tray = TrayIconBuilder::with_id("voiceinput-tray")
        .icon(icon)
        .menu(&menu)
        .tooltip("VoiceInput - 语音输入")
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "quit" => {
                    log::info!("Quit from tray menu");
                    app.exit(0);
                }
                _ if id.starts_with("device:") => {
                    let device_name = &id[7..];
                    log::info!("Selected audio device: {}", device_name);
                    // TODO: persist device selection to config
                }
                _ => {}
            }
        })
        .build(handle)?;

    Ok(())
}

fn build_tray_menu(handle: &AppHandle) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    // Version info (disabled)
    let version = MenuItemBuilder::with_id("version", "VoiceInput v1.0")
        .enabled(false)
        .build(handle)?;

    // Status indicator (disabled)
    let status = MenuItemBuilder::with_id("status", "✓ 已就绪")
        .enabled(false)
        .build(handle)?;

    // Audio device submenu
    let device_submenu = build_device_submenu(handle)?;

    // Auto-start toggle
    let auto_start = CheckMenuItemBuilder::with_id("auto_start", "开机自启")
        .checked(false)
        .build(handle)?;

    // Quit
    let quit = MenuItemBuilder::with_id("quit", "退出").build(handle)?;

    let menu = MenuBuilder::new(handle)
        .item(&version)
        .item(&status)
        .separator()
        .item(&device_submenu)
        .separator()
        .item(&auto_start)
        .separator()
        .item(&quit)
        .build()?;

    Ok(menu)
}

fn build_device_submenu(handle: &AppHandle) -> Result<tauri::menu::Submenu<tauri::Wry>, Box<dyn std::error::Error>> {
    let mut builder = SubmenuBuilder::with_id(handle, "devices", "音频设备");

    let host = cpal::default_host();
    let default_device_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let devices = host.input_devices().map_err(|e| format!("Failed to list devices: {}", e))?;

    let mut has_devices = false;
    for device in devices {
        if let Ok(name) = device.name() {
            let is_default = name == default_device_name;
            let id = format!("device:{}", name);
            let label = if is_default {
                format!("✓ {} (默认)", name)
            } else {
                name.clone()
            };
            let item = MenuItemBuilder::with_id(&id, &label).build(handle)?;
            builder = builder.item(&item);
            has_devices = true;
        }
    }

    if !has_devices {
        let no_device = MenuItemBuilder::with_id("no_device", "未检测到音频设备")
            .enabled(false)
            .build(handle)?;
        builder = builder.item(&no_device);
    }

    Ok(builder.build()?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Idle,
    Recording,
}

/// Update the tray icon to reflect the current state
pub fn update_tray_icon(handle: &AppHandle, state: TrayState) {
    let icon = generate_icon(state);
    // Try known tray ID first, then fallback to "voiceinput" ID
    if let Some(tray) = handle.tray_by_id("voiceinput-tray") {
        let _ = tray.set_icon(Some(icon));
    }
}

/// Generate a 32x32 RGBA icon programmatically
fn generate_icon(state: TrayState) -> Image<'static> {
    let size = ICON_SIZE as usize;
    let mut pixels = vec![0u8; size * size * 4];

    let (r, g, b) = match state {
        TrayState::Idle => (128u8, 128u8, 128u8),     // Gray
        TrayState::Recording => (239u8, 68u8, 68u8),   // Red
    };

    let center = size as f32 / 2.0;
    let radius = center - 2.0;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center + 0.5;
            let dy = y as f32 - center + 0.5;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = (y * size + x) * 4;

            if dist < radius {
                // Draw microphone shape inside circle
                let in_mic = is_mic_pixel(x as f32, y as f32, size as f32);
                if in_mic {
                    pixels[idx] = 255;     // R
                    pixels[idx + 1] = 255; // G
                    pixels[idx + 2] = 255; // B
                    pixels[idx + 3] = 255; // A
                } else {
                    pixels[idx] = r;
                    pixels[idx + 1] = g;
                    pixels[idx + 2] = b;
                    pixels[idx + 3] = 255;
                }
            }
        }
    }

    Image::new_owned(pixels, ICON_SIZE, ICON_SIZE)
}

/// Check if a pixel is part of a simple microphone icon shape
fn is_mic_pixel(x: f32, y: f32, size: f32) -> bool {
    let cx = size / 2.0;
    let cy = size / 2.0;

    // Normalize to -1..1 range relative to icon center
    let nx = (x - cx) / (size / 2.0);
    let ny = (y - cy) / (size / 2.0);

    // Mic body (rounded rectangle, top portion)
    let mic_width = 0.25;
    let mic_top = -0.55;
    let mic_bottom = 0.05;
    if nx.abs() < mic_width && ny > mic_top && ny < mic_bottom {
        return true;
    }

    // Mic head (semicircle on top)
    let head_cy = mic_top;
    let head_dist = ((nx * nx) + (ny - head_cy).powi(2)).sqrt();
    if head_dist < mic_width && ny < head_cy {
        return true;
    }

    // Mic arc (U-shape around the body)
    let arc_radius = 0.38;
    let arc_center_y = -0.1;
    let dist_from_arc = ((nx * nx) + (ny - arc_center_y).powi(2)).sqrt();
    if (dist_from_arc - arc_radius).abs() < 0.06 && ny > arc_center_y {
        return true;
    }

    // Stand (vertical line below)
    let stand_top = arc_center_y + arc_radius - 0.04;
    let stand_bottom = 0.55;
    if nx.abs() < 0.06 && ny > stand_top && ny < stand_bottom {
        return true;
    }

    // Base (horizontal line)
    if nx.abs() < 0.22 && (ny - stand_bottom).abs() < 0.06 {
        return true;
    }

    false
}
