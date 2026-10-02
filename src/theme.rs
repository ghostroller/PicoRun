/// Rendering consumes these values; search and catalog do not depend on them.
#[derive(Debug, Clone)]
pub struct Theme {
    pub background: Rgb,
    pub foreground: Rgb,
    pub selection: Rgb,
    pub muted: Rgb,
    pub font_family: String,
    pub font_size: u16,
    pub width: u16,
    pub row_height: u16,
    pub padding: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Rgb(24, 26, 30),
            foreground: Rgb(235, 237, 240),
            selection: Rgb(44, 65, 90),
            muted: Rgb(150, 155, 165),
            font_family: "Segoe UI".into(),
            font_size: 16,
            width: 560,
            row_height: 38,
            padding: 12,
        }
    }
}
