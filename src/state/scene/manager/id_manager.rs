pub struct IdManager
{
    material_id: u32,
    texture_id: u32,
    node_id: u32,
    instance_id: u32,
    camera_id: u32,
    light_id: u32,
}

impl IdManager
{
    pub fn new() -> IdManager
    {
        Self
        {
            material_id: 0,
            texture_id: 0,
            node_id: 0,
            instance_id: 0,
            camera_id: 0,
            light_id: 0
        }
    }

    pub fn get_next_material_id(&mut self) -> u32
    {
        self.material_id = self.material_id + 1;

        self.material_id
    }

    pub fn get_next_texture_id(&mut self) -> u32
    {
        self.texture_id = self.texture_id + 1;

        self.texture_id
    }

    pub fn get_next_node_id(&mut self) -> u32
    {
        self.node_id = self.node_id + 1;

        self.node_id
    }

    pub fn get_next_instance_id(&mut self) -> u32
    {
        self.instance_id = self.instance_id + 1;

        self.instance_id
    }

    pub fn get_next_camera_id(&mut self) -> u32
    {
        self.camera_id = self.camera_id + 1;

        self.camera_id
    }

    pub fn get_next_light_id(&mut self) -> u32
    {
        self.light_id = self.light_id + 1;

        self.light_id
    }

}