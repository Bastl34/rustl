pub struct IdManager
{
    material_id: u32,
    texture_id: u32,
    node_id: u32,
}

impl IdManager
{
    pub fn new() -> IdManager
    {
        Self
        {
            material_id: 0,
            texture_id: 0,
            node_id: 0
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

}