use iced::{Color, widget::image::Handle};
use image::{DynamicImage, EncodableLayout, ImageBuffer, Pixel, Rgb, Rgba};
use std::{array, mem};

pub fn process_image(original: DynamicImage, mut fg: Color, mut bg: Color) -> ImageHandles {
    let mut dark_pixels = 0;
    let mut bright_pixels = 0;
    let original = original.into_rgba8();
    let mut contains_color = false;
    for pixel in original.pixels() {
        match pixel.0 {
            [255, 255, 255, _] => bright_pixels += 1,
            [0, 0, 0, _] => dark_pixels += 1,
            _ => {
                if pixel.to_luma()[0] > 127 {
                    bright_pixels += 1;
                } else {
                    dark_pixels += 1;
                }

                if pixel[0] != pixel[1] || pixel[0] != pixel[2] {
                    contains_color = true
                }
            }
        }
    }

    if dark_pixels > bright_pixels {
        mem::swap(&mut fg, &mut bg);
    }

    let fg = Rgba::from(fg.into_rgba8());
    let bg = Rgba::from(bg.into_rgba8());
    let (width, height) = original.dimensions();
    let processed = ImageBuffer::from_par_fn(width, height, |x, y| {
        let pixel = original.get_pixel(x, y);
        match pixel.to_rgb().0 {
            [255, 255, 255] => bg,
            [0, 0, 0] => fg,
            _ => {
                let ratio = pixel.to_luma()[0] as f64 / 255.0;

                let array: [_; 3] = array::from_fn(|i| {
                    let fg = fg[i] as f64;
                    let bg = bg[i] as f64;

                    (bg * ratio + fg * (1.0 - ratio)) as u8
                });

                Rgb::from(array).to_rgba()
            }
        }
    });

    ImageHandles {
        processed: Handle::from_rgba(width, height, processed.as_bytes().to_vec()),
        original: Handle::from_rgba(width, height, original.as_bytes().to_vec()),
        contains_color,
    }
}

#[derive(Debug, Clone)]
pub struct ImageHandles {
    processed: Handle,
    original: Handle,
    contains_color: bool,
}

impl ImageHandles {
    pub fn get(&self, processing_enabled: bool) -> Handle {
        if processing_enabled {
            self.processed.clone()
        } else {
            self.original.clone()
        }
    }

    pub fn contains_color(&self) -> bool {
        self.contains_color
    }
}
