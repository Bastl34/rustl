
pub struct Consumable<T>
{
    changed: bool,
    data: T
}

impl<T> Consumable<T>
{
    pub fn new(data: T) -> Consumable<T>
    {
        Consumable::<T> { data, changed: false }
    }

    pub fn set(&mut self, data: T)
    {
        self.data = data;
        self.changed = true;
    }

    pub fn get(&self) -> &T
    {
        &self.data
    }

    pub fn changed(&self) -> bool
    {
        self.changed
    }

    pub fn consume_borrow(&mut self) -> (&T, bool)
    {
        let has_changed = self.changed;
        self.changed = false;

        (&self.data, has_changed)
    }

    pub fn consume(&mut self) -> (T, bool) where T: Copy
    {
        let has_changed = self.changed;
        self.changed = false;

        (self.data, has_changed)
    }
}