use std::sync::{RwLock, Arc};

use image::{DynamicImage, GenericImageView, Pixel};
use nalgebra::Vector4;

use crate::helper;

pub type TextureItem = Arc<RwLock<Box<Texture>>>;

#[derive(Debug)]
pub struct Texture
{
    pub id: u64,
    pub name: String,
    pub hash: String,

    pub image: DynamicImage,
}

impl Texture
{
    pub fn empty() -> Texture
    {
        Texture
        {
            id: 0,
            name: "empty".to_string(),
            hash: "".to_string(),

            image: DynamicImage::new_rgba8(0,0)
        }
    }

    pub fn new(id: u64, name: &str, image_bytes: &Vec<u8>) -> Texture
    {
        let image = image::load_from_memory(image_bytes.as_slice()).unwrap();
        let rgba = image.to_rgba8();


        let hash = helper::crypto::get_hash_from_byte_vec(image_bytes);

        Texture
        {
            id,
            name: name.to_string(),
            hash,

            image: image::DynamicImage::ImageRgba8(rgba)
        }
    }

    pub fn width(&self) -> u32
    {
        self.image.width()
    }

    pub fn height(&self) -> u32
    {
        self.image.width()
    }

    pub fn dimensions(&self) -> (u32, u32)
    {
        (self.image.width(), self.image.height())
    }

    pub fn get_pixel_as_float_vec(&self, x: u32, y: u32) -> Vector4<f32>
    {
        if self.width() == 0 && self.height() == 0
        {
            return Vector4::<f32>::new(0.0, 0.0, 0.0, 1.0);
        }

        let pixel = self.image.get_pixel(x, y);

        let rgba = pixel.to_rgba();

        Vector4::<f32>::new
        (
            (rgba[0] as f32) / 255.0,
            (rgba[1] as f32) / 255.0,
            (rgba[2] as f32) / 255.0,
            (rgba[3] as f32) / 255.0
        )
    }

    pub fn rgba_data(&self) -> &[u8]
    {
        self.image.as_bytes()
    }

}