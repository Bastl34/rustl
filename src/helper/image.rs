use image::{DynamicImage, ImageBuffer, RgbaImage, GenericImageView, Rgba, GrayImage, Luma};

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

pub fn float32_to_grayscale(img: DynamicImage) -> DynamicImage
{
    let mut converted: GrayImage = ImageBuffer::new(img.width(), img.height());

    // convert to rgba
    for x in 0..img.width()
    {
        for y in 0..img.height()
        {
            let pixel = img.get_pixel(x, y);
            let data = [[pixel[0], pixel[1], pixel[2], pixel[3]]];
            let float: f32 = bytemuck::cast(data);
            let u8: u8 = (float * 255.0) as u8;
            converted.put_pixel(x, y, Luma::<u8>([u8]));
        }
    }

    DynamicImage::ImageLuma8(converted)
}