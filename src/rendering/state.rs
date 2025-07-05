use crate::{rendering::{texture::{Texture, TextureFormat}, wgpu::WGpu}, state::{helper::render_item::get_render_item, scene::components::material::Material, state::State}};

pub fn update(wgpu: &mut WGpu, state: &mut State)
{
    update_textures(wgpu, state);
}

pub fn update_textures(wgpu: &mut WGpu, state: &mut State)
{
    // check all individual textures
    for (_texture_id, texture) in &mut state.textures
    {
        let mut buffer_recreate_needed = false;

        {
            let mut texture = texture.write().unwrap();
            let texture_changed = texture.get_data_mut().consume_change();

            // check if buffer recreation is needed
            // TODO: check if this even needed anymore (because of the changetracker data from texture)
            if let Some(render_item) = &texture.render_item
            {
                let render_item = get_render_item::<Texture>(render_item);
                buffer_recreate_needed = render_item.width != texture.width() || render_item.height != texture.height();
            }

            if texture.render_item.is_none() || buffer_recreate_needed || texture_changed
            {
                let mut format = TextureFormat::Srgba;
                if texture.channels() == 1
                {
                    format = TextureFormat::Gray;
                }

                let render_item = Texture::new_from_texture(wgpu, texture.name.as_str(), &texture, format);
                texture.render_item = Some(Box::new(render_item));
                buffer_recreate_needed = true;
            }
            /*
            else if texture_changed
            {
                let mut render_item = texture.render_item.take();

                {
                    let render_item = get_render_item_mut::<Texture>(render_item.as_mut().unwrap());
                    render_item.update_buffer(wgpu, &texture);
                }

                texture.render_item = render_item;
            }
                */
        }

        // mark material as "dirty" if the buffer needs a recreate
        if buffer_recreate_needed
        {
            let texture = texture.read().unwrap();

            for scene in &mut state.scenes
            {
                for (_, material) in &mut scene.materials
                {
                    let mut material = material.write().unwrap();
                    let material = material.as_any_mut().downcast_mut::<Material>().unwrap();

                    if material.has_texture_id(texture.id)
                    {
                        material.get_data_mut().force_change();
                    }
                }
            }
        }
    }
}