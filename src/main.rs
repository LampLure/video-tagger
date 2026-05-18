mod ai;
mod app;
mod audio;
mod cache;
mod config;
mod ffmpeg;
mod progress;
mod scanner;
mod tags;

use app::VideoTaggerApp;
use eframe::egui;
use std::fs;
use std::path::Path;

fn find_cjk_font() -> Option<Vec<u8>> {
    let win_fonts = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msyhbd.ttc",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsun.ttc",
        r"C:\Windows\Fonts\malgun.ttf",
    ];
    for path in &win_fonts {
        if let Ok(data) = fs::read(Path::new(path)) { return Some(data); }
    }
    let linux_fonts = [
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
        "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
    ];
    for path in &linux_fonts {
        if let Ok(data) = fs::read(Path::new(path)) { return Some(data); }
    }
    let macos_fonts = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/Library/Fonts/Noto Sans CJK TC Regular.otf",
    ];
    for path in &macos_fonts {
        if let Ok(data) = fs::read(Path::new(path)) { return Some(data); }
    }
    None
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Video Tagger",
        options,
        Box::new(|cc| {
            if let Some(cjk_font_data) = find_cjk_font() {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert("cjk".to_owned(), egui::FontData::from_owned(cjk_font_data).into());
                fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "cjk".to_owned());
                cc.egui_ctx.set_fonts(fonts);
            }
            Ok(Box::new(VideoTaggerApp::default()))
        }),
    )
}
