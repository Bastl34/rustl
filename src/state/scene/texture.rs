use std::{sync::{RwLock, Arc}, fmt::format};

use egui::WidgetWithState;
use image::{DynamicImage, GenericImageView, Pixel, ImageFormat, Rgba, ImageBuffer, imageops};
use nalgebra::Vector4;

use egui::Image;

use crate::{helper::{self, change_tracker::ChangeTracker}, state::helper::render_item::RenderItemOption};

pub type TextureItem = Arc<RwLock<Box<Texture>>>;

const PREVIEW_SIZE: u32 = 256;
const MAX_MIPMAPS: usize = 11; // max allowed mipmaps

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum TextureAddressMode
{
    ClampToEdge,
    Repeat,
    MirrorRepeat,
    ClampToBorder
}

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum TextureFilterMode
{
    Nearest,
    Linear
}

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum MipmapSamplingFilterType
{
    Nearest,
    Triangle,
    CatmullRom,
    Gaussian,
    Lanczos3,
}

pub struct TextureData
{
    pub preview: DynamicImage,
    pub image: DynamicImage,
    pub egui_preview: Option<egui::TextureHandle>,

    pub width: u64,
    pub height: u64,

    pub mipmapping: bool,

    pub mipmap_sampling_type: MipmapSamplingFilterType,

    pub has_transparency: bool, // if there is a pixel with a alpha value < 1.0

    pub address_mode_u: TextureAddressMode,
    pub address_mode_v: TextureAddressMode,
    pub address_mode_w: TextureAddressMode,
    pub mag_filter: TextureFilterMode,
    pub min_filter: TextureFilterMode,
    pub mipmap_filter: TextureFilterMode,
}

pub struct Texture
{
    pub id: u64,
    pub name: String,
    pub hash: String, // this is mainly used for initial loading and to check if there is a texture already loaded (in dynamic textires - this may does not get updates)

    pub data: ChangeTracker<TextureData>,

    pub render_item: RenderItemOption
}

impl Texture
{
    pub fn empty() -> Texture
    {
        let data: TextureData = TextureData
        {
            preview: DynamicImage::new_rgba8(0,0),
            image: DynamicImage::new_rgba8(0,0),
            egui_preview: None,

            width: 0,
            height: 0,

            mipmapping: true,

            mipmap_sampling_type: MipmapSamplingFilterType::Triangle,

            has_transparency: false,

            address_mode_u: TextureAddressMode::ClampToEdge,
            address_mode_v: TextureAddressMode::ClampToEdge,
            address_mode_w: TextureAddressMode::ClampToEdge,
            mag_filter: TextureFilterMode::Linear,
            min_filter: TextureFilterMode::Nearest,
            mipmap_filter: TextureFilterMode::Nearest
        };

        Texture
        {
            id: 0,
            name: "empty".to_string(),
            hash: "".to_string(),

            data: ChangeTracker::new(data),

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

        let has_transparency = rgba.enumerate_pixels().find(|pixel| { pixel.2[3] < 255 }).is_some();

        let image = image::DynamicImage::ImageRgba8(rgba);

        let data: TextureData = TextureData
        {
            width: image.width() as u64,
            height: image.height() as u64,

            mipmapping: true,

            mipmap_sampling_type: MipmapSamplingFilterType::Triangle,

            has_transparency: has_transparency,

            preview: Self::create_preview(&image),
            image: image,
            egui_preview: None,

            address_mode_u: TextureAddressMode::ClampToEdge,
            address_mode_v: TextureAddressMode::ClampToEdge,
            address_mode_w: TextureAddressMode::ClampToEdge,
            mag_filter: TextureFilterMode::Linear,
            min_filter: TextureFilterMode::Nearest,
            mipmap_filter: TextureFilterMode::Nearest
        };

        Texture
        {
            id,
            name: name.to_string(),
            hash,

            data: ChangeTracker::new(data),

            render_item: None
        }
    }

    pub fn new_from_image_channel(id: u64, name: &str, texture: &Texture, channel: usize) -> Texture
    {
        let width = texture.width();
        let height = texture.height();

        let mut image = ImageBuffer::new(width, height);

        let data = texture.get_data();

        for x in 0..width
        {
            for y in 0..height
            {
                let pixel = data.image.get_pixel(x, y);
                image.put_pixel(x, y, Rgba([pixel[channel], 0 , 0, 255]));
            }
        }

        let bytes = &image.to_vec();
        let hash = helper::crypto::get_hash_from_byte_vec(&bytes);

        let image = image::DynamicImage::ImageRgba8(image);

        let data: TextureData = TextureData
        {
            width: image.width() as u64,
            height: image.height() as u64,

            has_transparency: false,

            mipmapping: true,

            mipmap_sampling_type: MipmapSamplingFilterType::Triangle,

            preview: Self::create_preview(&image),
            image: image,
            egui_preview: None,

            address_mode_u: TextureAddressMode::ClampToEdge,
            address_mode_v: TextureAddressMode::ClampToEdge,
            address_mode_w: TextureAddressMode::ClampToEdge,
            mag_filter: TextureFilterMode::Linear,
            min_filter: TextureFilterMode::Nearest,
            mipmap_filter: TextureFilterMode::Nearest
        };

        Texture
        {
            id,
            name: name.to_string(),
            hash,

            data: ChangeTracker::new(data),

            render_item: None
        }
    }

    pub fn create_mipmap_levels(&self) -> Vec<DynamicImage>
    {
        let mut mipmaps = Vec::new();

        let mut current_level = self.get_data().image.clone();
        loop
        {
            let width = current_level.width() / 2;
            let height = current_level.height() / 2;

            let filter_method;
            match self.get_data().mipmap_sampling_type
            {
                MipmapSamplingFilterType::Nearest => filter_method = imageops::FilterType::Nearest,
                MipmapSamplingFilterType::Triangle => filter_method = imageops::FilterType::Triangle,
                MipmapSamplingFilterType::CatmullRom => filter_method = imageops::FilterType::CatmullRom,
                MipmapSamplingFilterType::Gaussian => filter_method = imageops::FilterType::Gaussian,
                MipmapSamplingFilterType::Lanczos3 => filter_method = imageops::FilterType::Lanczos3,
            }

            current_level = image::DynamicImage::ImageRgba8(imageops::resize(&current_level, width, height, filter_method));

            if current_level.width() >= 1 && current_level.height() >= 1 && mipmaps.len() < MAX_MIPMAPS
            {
                mipmaps.push(current_level.clone());
            }
            else
            {
                break;
            }
        }

        mipmaps
    }

    pub fn create_preview(image: &DynamicImage) -> DynamicImage
    {
        if image.width() < PREVIEW_SIZE && image.height() < PREVIEW_SIZE
        {
            return image.clone();
        }

        let mut width = PREVIEW_SIZE;
        let mut height = ((PREVIEW_SIZE as f32) * image.height() as f32 / image.width() as f32).floor() as u32;
        if image.height() > image.width()
        {
            height = PREVIEW_SIZE;
            width = ((PREVIEW_SIZE as f32) * image.width() as f32 / image.height() as f32).floor() as u32;
        }

        let preview_filter = imageops::FilterType::Gaussian;
        image.resize(width, height, preview_filter)
    }

    pub fn get_data(&self) -> &TextureData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<TextureData>
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<TextureData>
    {
        &mut self.data
    }

    pub fn get_dynamic_image(&self) -> &DynamicImage
    {
        &self.get_data().image
    }

    pub fn get_dynamic_image_mut(&mut self) -> &mut DynamicImage
    {
        &mut self.get_data_mut().get_mut().image
    }

    pub fn width(&self) -> u32
    {
        self.data.get_ref().image.width()
    }

    pub fn height(&self) -> u32
    {
        self.data.get_ref().image.height()
    }

    pub fn channels(&self) -> u32
    {
        let image = &self.data.get_ref().image;

        if image.width() == 0 || image.height() == 0
        {
            return 0;
        }

        image.get_pixel(0, 0).channels().len() as u32
    }

    pub fn memory_usage(&self) -> u64
    {
        // image
        let mut bytes = self.get_data().width * self.get_data().height * self.channels() as u64;

        // preview
        bytes += self.get_data().preview.width() as u64 * self.get_data().preview.width() as u64 * 4;

        bytes
    }

    pub fn gpu_usage(&self) -> u64
    {
        if self.render_item.is_none()
        {
            return 0;
        }

        // gpu memory: 4 channels
        let mut bytes = self.get_data().width * self.get_data().height * 4;

        // mipmaps are using around + 1/3 more gpu memory --> https://en.wikipedia.org/wiki/Mipmap
        if self.get_data().mipmapping
        {
            bytes += (bytes as f32 * (1.0 / 3.0)).round() as u64;
        }

        if self.get_data().egui_preview.is_some()
        {
            bytes += self.get_data().preview.width() as u64 * self.get_data().preview.width() as u64 * 4;
        }

        bytes
    }

    pub fn dimensions(&self) -> (u32, u32)
    {
        (self.data.get_ref().image.width(), self.data.get_ref().image.height())
    }

    pub fn get_pixel_as_float_vec(&self, x: u32, y: u32) -> Vector4<f32>
    {
        if self.width() == 0 && self.height() == 0
        {
            return Vector4::<f32>::new(0.0, 0.0, 0.0, 1.0);
        }

        let pixel = self.data.get_ref().image.get_pixel(x, y);

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
        self.data.get_ref().image.as_bytes()
    }

    pub fn create_egui_preview(&mut self, ctx: &egui::Context)
    {
        if self.get_data().egui_preview.is_some()
        {
            return;
        }

        let name = format!("{}_preview",self.name);

        let data = self.get_data_mut();
        let data = data.get_unmarked_mut();

        let pixels = data.preview.as_flat_samples_u8();
        let pixels = pixels.unwrap();

        let color_image = egui::ColorImage::from_rgba_unmultiplied
        (
            [data.preview.width() as usize, data.preview.height() as usize],
            pixels.as_slice()
        );

        let texture = ctx.load_texture(name, color_image, Default::default());

        data.egui_preview = Some(texture);
    }

    pub fn ui_info(&mut self, ui: &mut egui::Ui)
    {
        {
            self.create_egui_preview(ui.ctx());
        }

        let data = self.get_data();

        if let Some(preview) = &data.egui_preview
        {
            ui.image((preview.id(), preview.size_vec2()));
        }

        ui.horizontal(|ui|
        {
            ui.label("Resolution: ");
            ui.label(format!("{}x{}", data.width, data.height));
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui)
    {
        let mut mipmapping;

        let mut mipmap_sampling_type;

        let mut address_mode_u;
        let mut address_mode_v;
        let mut address_mode_w;
        let mut mag_filter;
        let mut min_filter;
        let mut mipmap_filter;

        {
            let data = self.data.get_ref();

            mipmapping = data.mipmapping;

            mipmap_sampling_type = data.mipmap_sampling_type;

            address_mode_u = data.address_mode_u;
            address_mode_v = data.address_mode_v;
            address_mode_w = data.address_mode_w;
            mag_filter = data.mag_filter;
            min_filter = data.min_filter;
            mipmap_filter = data.mipmap_filter;
        }

        let mut apply_settings = false;

        apply_settings = ui.checkbox(&mut mipmapping, "use mipmap").changed() || apply_settings;

        ui.horizontal(|ui|
        {
            ui.label("Mipmap Sample Type:");

            egui::ComboBox::from_id_source(ui.make_persistent_id("mipmap_sampling_type")).selected_text(format!("{mipmap_sampling_type:?}")).show_ui(ui, |ui|
            {
                apply_settings = ui.selectable_value(& mut mipmap_sampling_type, MipmapSamplingFilterType::Nearest, "Nearest").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut mipmap_sampling_type, MipmapSamplingFilterType::Triangle, "Triangle").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut mipmap_sampling_type, MipmapSamplingFilterType::Lanczos3, "Lanczos3").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut mipmap_sampling_type, MipmapSamplingFilterType::Gaussian, "Gaussian").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut mipmap_sampling_type, MipmapSamplingFilterType::CatmullRom, "CatmullRom").changed() || apply_settings;
            });
        });


        ui.horizontal(|ui|
        {
            ui.label("Address Mode U:");

            egui::ComboBox::from_id_source(ui.make_persistent_id("address_mode_u")).selected_text(format!("{address_mode_u:?}")).show_ui(ui, |ui|
            {
                apply_settings = ui.selectable_value(& mut address_mode_u, TextureAddressMode::ClampToBorder, "ClampToBorder").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut address_mode_u, TextureAddressMode::ClampToEdge, "ClampToEdge").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut address_mode_u, TextureAddressMode::MirrorRepeat, "MirrorRepeat").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut address_mode_u, TextureAddressMode::Repeat, "Repeat").changed() || apply_settings;
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Address Mode V:");

            egui::ComboBox::from_id_source(ui.make_persistent_id("address_mode_v")).selected_text(format!("{address_mode_v:?}")).show_ui(ui, |ui|
            {
                apply_settings = ui.selectable_value(& mut address_mode_v, TextureAddressMode::ClampToBorder, "ClampToBorder").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut address_mode_v, TextureAddressMode::ClampToEdge, "ClampToEdge").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut address_mode_v, TextureAddressMode::MirrorRepeat, "MirrorRepeat").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut address_mode_v, TextureAddressMode::Repeat, "Repeat").changed() || apply_settings;
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Address Mode W:");

            egui::ComboBox::from_id_source(ui.make_persistent_id("address_mode_w")).selected_text(format!("{address_mode_w:?}")).show_ui(ui, |ui|
            {
                apply_settings = ui.selectable_value(& mut address_mode_w, TextureAddressMode::ClampToBorder, "ClampToBorder").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut address_mode_w, TextureAddressMode::ClampToEdge, "ClampToEdge").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut address_mode_w, TextureAddressMode::MirrorRepeat, "MirrorRepeat").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut address_mode_w, TextureAddressMode::Repeat, "Repeat").changed() || apply_settings;
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Mag Filter: ");

            egui::ComboBox::from_id_source(ui.make_persistent_id("mag_filter")).selected_text(format!("{mag_filter:?}")).show_ui(ui, |ui|
            {
                apply_settings = ui.selectable_value(& mut mag_filter, TextureFilterMode::Linear, "Linear").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut mag_filter, TextureFilterMode::Nearest, "Nearest").changed() || apply_settings;
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Min Filter: ");

            egui::ComboBox::from_id_source(ui.make_persistent_id("min_filter")).selected_text(format!("{min_filter:?}")).show_ui(ui, |ui|
            {
                apply_settings = ui.selectable_value(& mut min_filter, TextureFilterMode::Linear, "Linear").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut min_filter, TextureFilterMode::Nearest, "Nearest").changed() || apply_settings;
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Mipmap Filter: ");

            egui::ComboBox::from_id_source(ui.make_persistent_id("mipmap_filter")).selected_text(format!("{mipmap_filter:?}")).show_ui(ui, |ui|
            {
                apply_settings = ui.selectable_value(& mut mipmap_filter, TextureFilterMode::Linear, "Linear").changed() || apply_settings;
                apply_settings = ui.selectable_value(& mut mipmap_filter, TextureFilterMode::Nearest, "Nearest").changed() || apply_settings;
            });
        });

        if apply_settings
        {
            let data = self.get_data_mut().get_mut();

            data.mipmapping = mipmapping;

            data.mipmap_sampling_type = mipmap_sampling_type;

            data.address_mode_u = address_mode_u;
            data.address_mode_v = address_mode_v;
            data.address_mode_w = address_mode_w;
            data.mag_filter = mag_filter;
            data.min_filter = min_filter;
            data.mipmap_filter = mipmap_filter;
        }
    }

}