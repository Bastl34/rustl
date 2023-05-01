use image::{DynamicImage, ImageBuffer, RgbaImage, GenericImageView, Rgba};

pub fn brga_to_rgba(img: DynamicImage) -> DynamicImage
{
    let mut converted: RgbaImage = ImageBuffer::new(img.width(), img.height());

    // convert to rgba
    for x in 0..img.width()
    {
        for y in 0..img.height()
        {
            let pixel = img.get_pixel(x, y);
            converted.put_pixel(x, y, Rgba([pixel[2], pixel[1], pixel[0], pixel[3]]));
        }
    }

    DynamicImage::ImageRgba8(converted)
}