use std::io::Cursor;

use cosmic_text::{SwashCache, FontSystem, Metrics, Attrs, Buffer, Color, Family, BufferLine, AttrsList};
use image::{ImageBuffer, Rgba, ImageOutputFormat};
use palette::blend::Compose;
use syntect::{parsing::{SyntaxReference, SyntaxSet}, highlighting::ThemeSet, util::LinesWithEndings};

pub type PngData = Vec<u8>;

pub fn draw_code(mut font_system: &mut FontSystem, swash_cache: &mut SwashCache, syntax_set: &SyntaxSet, theme_set: &ThemeSet, code: &str, syntax: &SyntaxReference) -> PngData {
    let metrics = Metrics::new(32.0, 44.0).scale(1.5);
    let mut buffer = Buffer::new(&mut font_system, metrics);
    let mut buffer = buffer.borrow_with(font_system);

    buffer.set_size(800.0, 250.0);

    let default_text_color = Color::rgb(255,0,255); // magenta: should not appear
    let attrs = Attrs::new().color(default_text_color);
    let mono_attrs = attrs.family(Family::Monospace);


    let theme = &theme_set.themes["base16-ocean.dark"];
    let mut h = syntect::easy::HighlightLines::new(syntax, theme);

    fn color_syntect_to_cosmic(c: syntect::highlighting::Color) -> cosmic_text::Color {
        cosmic_text::Color::rgba(c.r, c.g, c.b, c.a)
    }
    fn color_syntect_to_palette(c: syntect::highlighting::Color) -> palette::LinSrgba {
        palette::LinSrgba::new(c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0, c.a as f32 / 255.0)
    }


    for line in LinesWithEndings::from(code) {
        let ranges = h.highlight_line(line, syntax_set).expect("Failed to parse line to highlight?");
        let line_parts = ranges.iter().map(|(style, text)| (text, mono_attrs.color(color_syntect_to_cosmic(style.foreground))));
        let mut attrs_list = AttrsList::new(mono_attrs);
        let mut cursor_pos = 0;
        for (text, attrs) in line_parts {
            let start = cursor_pos;
            cursor_pos += text.len();
            let end = cursor_pos;
            attrs_list.add_span(start..end, attrs);
        }

        buffer.lines.push(BufferLine::new(line, attrs_list));
    }


    buffer.shape_until_scroll();
    
    let (wf,hf) = buffer.size();
    let buf_width = wf.ceil() as u32;
    let buf_height = hf.ceil() as u32;

   let palette_default_pixel = color_syntect_to_palette(theme.settings.background.unwrap_or(syntect::highlighting::Color::BLACK));

    let mut palette_buffer: Vec<palette::Alpha<palette::rgb::Rgb<palette::encoding::Linear<palette::encoding::Srgb>, f32>, f32>> = vec![palette_default_pixel; (buf_width * buf_height) as usize];

    buffer.draw(swash_cache, default_text_color, |x,y,w,h,color| {
        let w = w as i32;
        let h = h as i32;
        let pixel = palette::LinSrgba::new(
            color.r() as f32 / 255.0, color.g() as f32 / 255.0, color.b() as f32 / 255.0, color.a() as f32 / 255.0);
        
        // This is an acceptable implementation of rectangle drawing,
        // because in practice most of the rectangles being drawn are 1x1 pixel.

        for px in x..x+w {
            for py in y..y+h {
                let px = px.clamp(0, buf_width as i32) as usize;
                let py = py.clamp(0, buf_height as i32) as usize;
                let pos = px + py*(buf_width as usize);

                let old_pixel = palette_buffer[pos];
                palette_buffer[pos] = pixel.over(old_pixel);
            }
        }
    });


    let default_pixel = Rgba([255,0,255,0]);
    let mut img_buffer = ImageBuffer::from_pixel(buf_width, buf_height, default_pixel);
    for (pos, value) in palette_buffer.iter().enumerate() {
        let y = pos / (buf_width as usize);
        let x = pos % (buf_width as usize);
        let r = (value.red*255.0) as u8;
        let g = (value.green*255.0) as u8;
        let b = (value.blue*255.0) as u8;
        let a = (value.alpha*255.0) as u8;
        img_buffer.put_pixel(x as u32, y as u32, Rgba([r,g,b,a]));
    }


    let mut out = PngData::new();
    img_buffer.write_to(&mut Cursor::new(&mut out), ImageOutputFormat::Png).expect("Encoding drawing into PNG in memory should be infallible");
    out

}