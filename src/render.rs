use std::io::Cursor;

use cosmic_text::{SwashCache, FontSystem, Metrics, Attrs, Buffer, Color, Family, BufferLine, AttrsList};
use image::{ImageBuffer, Rgba, ImageOutputFormat};
use syntect::{parsing::SyntaxSet, highlighting::ThemeSet};

pub type PngData = Vec<u8>;

pub fn draw_code(mut font_system: &mut FontSystem, swash_cache: &mut SwashCache, syntax_set: &SyntaxSet, theme_set: &ThemeSet, code: &str) -> PngData {
    let metrics = Metrics::new(32.0, 44.0).scale(1.5);
    let mut buffer = Buffer::new(&mut font_system, metrics);
    let mut buffer = buffer.borrow_with(font_system);

    buffer.set_size(800.0, 250.0);

    let default_text_color = Color::rgb(255, 255, 255);
    let attrs = Attrs::new().color(default_text_color);
    let mono_attrs = attrs.family(Family::Monospace);


    for line in code.split("\n") {
        buffer.lines.push(BufferLine::new(line, AttrsList::new(mono_attrs.color(Color::rgb(255, 0, 0)))));
    }

    buffer.shape_until_scroll();
    
    let (wf,hf) = buffer.size();
    let buf_width = wf.ceil() as u32;
    let buf_height = hf.ceil() as u32;

    let default_pixel = Rgba::<u8>::from([0,0,0,255]);
    let mut img_buffer = ImageBuffer::from_pixel(buf_width, buf_height, default_pixel);

    buffer.draw(swash_cache, default_text_color, |x,y,w,h,color| {
        let w = w as i32;
        let h = h as i32;
        let color = [color.r(), color.g(), color.b(), color.a()];
        let pixel = Rgba(color);
        // This is an acceptable implementation of rectangle drawing,
        // because in practice most of the rectangles being drawn are 1x1 pixel.

        for px in x..x+w {
            for py in y..y+h {
                let px = px.clamp(0, buf_width as i32) as u32;
                let py = py.clamp(0, buf_height as i32) as u32;

                let old_pixel = img_buffer.get_pixel(px, py);
                // Perform alpha blending
                // Using https://stackoverflow.com/a/12016968/5936187
                let bg = old_pixel.0;
                let fg = pixel.0;
                let alpha = (fg[3] as u16) + 1;
                let inv_alpha = 256 - (fg[0] as u16);
                let new_pixel = [
                    ((alpha * fg[0] as u16 + inv_alpha * bg[0] as u16) >> 8) as u8,
                    ((alpha * fg[1] as u16 + inv_alpha * bg[1] as u16) >> 8) as u8,
                    ((alpha * fg[2] as u16 + inv_alpha * bg[2] as u16) >> 8) as u8,
                    255
                ];

                img_buffer.put_pixel(px, py, Rgba(new_pixel));
            }
        }
    });


    let mut out = PngData::new();
    img_buffer.write_to(&mut Cursor::new(&mut out), ImageOutputFormat::Png).expect("Encoding drawing into PNG in memory should be infallible");
    out

}