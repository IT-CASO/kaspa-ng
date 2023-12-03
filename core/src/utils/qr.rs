use crate::imports::*;
use qrcode::render::svg;
use qrcode::*;

pub fn render_qrcode(text: &str, width: usize, height: usize) -> String {
    let code = QrCode::with_version(text, Version::Normal(4), EcLevel::L).unwrap();

    // let _theme = theme();

    code.render::<svg::Color<'_>>()
        .min_dimensions(width as u32, height as u32)
        .light_color(svg::Color(theme_color().qr_background.to_hex().as_str()))
        .dark_color(svg::Color(theme_color().qr_foreground.to_hex().as_str()))
        .build()
        .to_string()
}
