use std::sync::{RwLock, Arc};

use image::{DynamicImage, GenericImageView, Pixel, ImageFormat, imageops, GrayImage};
use nalgebra::Vector4;

use crate::{helper::{self, change_tracker::ChangeTracker}, state::helper::render_item::RenderItemOption};

pub type TextureItem = Arc<RwLock<Box<Texture>>>;

const PREVIEW_SIZE: u32 = 256;
const MAX_MIPMAPS: usize = 10; // max allowed mipmaps 10 (+ original texture)

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

    pub egui_preview: Option<egui::TextureHandle>,
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

            width: 0,
            height: 0,

            mipmapping: false,

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

            egui_preview: None,
            render_item: None
        }
    }

    pub fn new(id: u64, name: &str, image_bytes: &Vec<u8>, extension: Option<String>, max_resolution: u32) -> Texture
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

        let mut image = image::DynamicImage::ImageRgba8(rgba);

        if max_resolution > 0
        {
            if let Some(resized) = Self::resize_based_on_max_resolution(&image, max_resolution)
            {
                image = resized;
            }
        }

        let data: TextureData = TextureData
        {
            width: image.width() as u64,
            height: image.height() as u64,

            mipmapping: false,

            mipmap_sampling_type: MipmapSamplingFilterType::Triangle,

            has_transparency: has_transparency,

            preview: Self::create_preview(&image),
            image: image,

            address_mode_u: TextureAddressMode::ClampToEdge,
            address_mode_v: TextureAddressMode::ClampToEdge,
            address_mode_w: TextureAddressMode::ClampToEdge,
            mag_filter: TextureFilterMode::Linear,
            min_filter: TextureFilterMode::Linear,
            mipmap_filter: TextureFilterMode::Linear
        };

        Texture
        {
            id,
            name: name.to_string(),
            hash,

            data: ChangeTracker::new(data),

            egui_preview: None,
            render_item: None
        }
    }

    pub fn new_from_image_channel(id: u64, name: &str, texture: &Texture, channel: usize, max_resolution: u32) -> Texture
    {
        let width = texture.width();
        let height = texture.height();

        let mut image = GrayImage::new(width, height);

        let data = texture.get_data();

        let rgba = data.image.to_rgba8();

        for (x, y, px) in rgba.enumerate_pixels()
        {
            image[(x, y)][0] = px[channel];
        }

        let bytes = &image.to_vec();
        let hash = helper::crypto::get_hash_from_byte_vec(&bytes);

        let mut image = image::DynamicImage::ImageLuma8(image);

        if max_resolution > 0
        {
            if let Some(resized) = Self::resize_based_on_max_resolution(&image, max_resolution)
            {
                image = resized;
            }
        }

        let data: TextureData = TextureData
        {
            width: image.width() as u64,
            height: image.height() as u64,

            has_transparency: false,

            mipmapping: false,

            mipmap_sampling_type: MipmapSamplingFilterType::Triangle,

            preview: Self::create_preview(&image),
            image: image,

            address_mode_u: TextureAddressMode::ClampToEdge,
            address_mode_v: TextureAddressMode::ClampToEdge,
            address_mode_w: TextureAddressMode::ClampToEdge,
            mag_filter: TextureFilterMode::Linear,
            min_filter: TextureFilterMode::Linear,
            mipmap_filter: TextureFilterMode::Linear
        };

        Texture
        {
            id,
            name: name.to_string(),
            hash,

            data: ChangeTracker::new(data),

            egui_preview: None,
            render_item: None
        }
    }

    pub fn create_mipmap_levels(&self) -> Vec<DynamicImage>
    {
        let filter_method;
        match self.get_data().mipmap_sampling_type
        {
            MipmapSamplingFilterType::Nearest => filter_method = imageops::FilterType::Nearest,
            MipmapSamplingFilterType::Triangle => filter_method = imageops::FilterType::Triangle,
            MipmapSamplingFilterType::CatmullRom => filter_method = imageops::FilterType::CatmullRom,
            MipmapSamplingFilterType::Gaussian => filter_method = imageops::FilterType::Gaussian,
            MipmapSamplingFilterType::Lanczos3 => filter_method = imageops::FilterType::Lanczos3,
        }

        let mut mipmaps = Vec::new();

        let mut current_level = self.get_data().image.clone();
        loop
        {
            let width = current_level.width() / 2;
            let height = current_level.height() / 2;

            current_level = current_level.resize(width, height, filter_method);

            if width >= 1 && height >= 1 && mipmaps.len() < MAX_MIPMAPS
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

    pub fn resize_based_on_max_resolution(image: &DynamicImage, max_resolution: u32) -> Option<DynamicImage>
    {
        if image.width() < max_resolution && image.height() < max_resolution
        {
            return None;
        }

        let mut factor = max_resolution as f32 / image.width() as f32;
        if image.height() > image.width()
        {
            factor = max_resolution as f32 / image.height() as f32;
        }

        let width = (image.width() as f32 * factor).round() as u32;
        let height = (image.height() as f32 * factor).round() as u32;

        let filter = imageops::FilterType::Triangle;
        let resized = image.resize(width, height, filter);

        Some(resized)
    }

    pub fn get_mipmap_levels_amount(&self) -> usize
    {
        if !self.get_data().mipmapping
        {
            return 1;
        }

        let mut current_width = self.width();
        let mut current_height = self.height();

        let mut levels = 0;

        loop
        {
            current_width = current_width / 2;
            current_height = current_height / 2;

            if current_width >= 1 && current_height >= 1 && levels < MAX_MIPMAPS
            {
                levels += 1;
            }
            else
            {
                break;
            }
        }

        levels + 1 // add 1 for level=0 which is the full res
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
        self.get_data().image.color().channel_count() as u32
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

        let mut bytes = self.get_data().width * self.get_data().height * self.channels() as u64;

        // mipmaps are using around + 1/3 more gpu memory --> https://en.wikipedia.org/wiki/Mipmap
        if self.get_data().mipmapping
        {
            bytes += (bytes as f32 * (1.0 / 3.0)).round() as u64;
        }

        if self.egui_preview.is_some()
        {
            bytes += self.get_data().preview.width() as u64 * self.get_data().preview.width() as u64 * self.channels() as u64;
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

    pub fn raw_data(&self) -> &[u8]
    {
        self.data.get_ref().image.as_bytes()
    }

    pub fn create_egui_preview(&mut self, ctx: &egui::Context)
    {
        if self.egui_preview.is_some()
        {
            return;
        }

        let name = format!("{}_preview",self.name);

        let data = self.get_data();

        let pixels = data.preview.as_flat_samples_u8();
        let pixels = pixels.unwrap();

        let image;
        if self.channels() == 1
        {
            image = egui::ColorImage::from_gray([data.preview.width() as usize, data.preview.height() as usize], pixels.as_slice());
        }
        else
        {
            image = egui::ColorImage::from_rgba_unmultiplied([data.preview.width() as usize, data.preview.height() as usize], pixels.as_slice());
        }


        let texture = ctx.load_texture(name, image, Default::default());

        self.egui_preview = Some(texture);
    }

    pub fn ui_info(&mut self, ui: &mut egui::Ui)
    {
        {
            self.create_egui_preview(ui.ctx());
        }

        let data = self.get_data();

        if let Some(preview) = &self.egui_preview
        {
            ui.image((preview.id(), preview.size_vec2()));
        }

        let gpu_size = self.gpu_usage() as f32 / 1024.0 / 1024.0;

        let format = if self.channels() == 1 { "Gray" } else { "RGBA" };

        ui.label(format!("{}x{}, {}, {} mips, {:.2} MB", data.width, data.height, format, self.get_mipmap_levels_amount(), gpu_size));
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