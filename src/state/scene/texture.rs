use std::sync::{RwLock, Arc};

use image::{DynamicImage, GenericImageView, Pixel, ImageFormat, Rgba, ImageBuffer};
use nalgebra::Vector4;

use crate::{helper, state::helper::render_item::RenderItemOption};

pub type TextureItem = Arc<RwLock<Box<Texture>>>;

pub enum TextureAddressMode
{
    ClampToEdge,
    Repeat,
    MirrorRepeat,
    ClampToBorder
}

pub enum TextureFilterMode
{
    Nearest,
    Linear
}

pub struct Texture
{
    pub id: u64,
    pub name: String,
    pub hash: String,
    pub image: DynamicImage,

    pub address_mode_u: TextureAddressMode,
    pub address_mode_v: TextureAddressMode,
    pub address_mode_w: TextureAddressMode,
    pub mag_filter: TextureFilterMode,
    pub min_filter: TextureFilterMode,
    pub mipmap_filter: TextureFilterMode,

    pub render_item: RenderItemOption
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

            image: DynamicImage::new_rgba8(0,0),

            address_mode_u: TextureAddressMode::ClampToEdge,
            address_mode_v: TextureAddressMode::ClampToEdge,
            address_mode_w: TextureAddressMode::ClampToEdge,
            mag_filter: TextureFilterMode::Linear,
            min_filter: TextureFilterMode::Nearest,
            mipmap_filter: TextureFilterMode::Nearest,

            render_item: None
        }
    }

    pub fn new(id: u64, name: &str, image_bytes: &Vec<u8>, extension: Option<String>) -> Texture
    {
        let image;

        if let Some(extension) = extension
        {
            let format = ImageFormat::from_extension(extension).unwrap();
            image = image::load_from_memory_with_format(image_bytes.as_slice(), format).unwrap();
        }
        else
        {
            image = image::load_from_memory(image_bytes.as_slice()).unwrap();
        }

        let rgba = image.to_rgba8();

        let hash = helper::crypto::get_hash_from_byte_vec(image_bytes);
        //let hash = helper::crypto::get_hash_from_byte_vec(&rgba.to_vec());

        Texture
        {
            id,
            name: name.to_string(),
            hash,

            image: image::DynamicImage::ImageRgba8(rgba),

            address_mode_u: TextureAddressMode::ClampToEdge,
            address_mode_v: TextureAddressMode::ClampToEdge,
            address_mode_w: TextureAddressMode::ClampToEdge,
            mag_filter: TextureFilterMode::Linear,
            min_filter: TextureFilterMode::Nearest,
            mipmap_filter: TextureFilterMode::Nearest,

            render_item: None
        }
    }

    pub fn new_from_image_channel(id: u64, name: &str, texture: &Texture, channel: usize) -> Texture
    {
        let width = texture.width();
        let height = texture.height();

        let mut image = ImageBuffer::new(width, height);

        for x in 0..width
        {
            for y in 0..height
            {
                let pixel = texture.image.get_pixel(x, y);
                image.put_pixel(x, y, Rgba([pixel[channel], 0 , 0, 0]));
            }
        }

        let bytes = &image.to_vec();
        let hash = helper::crypto::get_hash_from_byte_vec(&bytes);

        Texture
        {
            id,
            name: name.to_string(),
            hash,

            image: image::DynamicImage::ImageRgba8(image),

            address_mode_u: TextureAddressMode::ClampToEdge,
            address_mode_v: TextureAddressMode::ClampToEdge,
            address_mode_w: TextureAddressMode::ClampToEdge,
            mag_filter: TextureFilterMode::Linear,
            min_filter: TextureFilterMode::Nearest,
            mipmap_filter: TextureFilterMode::Nearest,

            render_item: None
        }
    }

    pub fn get_dynamic_image(&self) -> &DynamicImage
    {
        &self.image
    }

    pub fn get_dynamic_image_mut(&mut self) -> &mut DynamicImage
    {
        &mut self.image
    }

    pub fn width(&self) -> u32
    {
        self.image.width()
    }

    pub fn height(&self) -> u32
    {
        self.image.height()
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