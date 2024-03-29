#![allow(dead_code)]

pub struct ChangeTracker<T>
{
    changed: bool,
    data: T
}

impl<T> ChangeTracker<T>
{
    pub fn new(data: T) -> ChangeTracker<T>
    {
        ChangeTracker::<T> { data, changed: false }
    }

    pub fn set(&mut self, data: T)
    {
        self.data = data;
        self.changed = true;
    }

    pub fn force_change(&mut self)
    {
        self.changed = true;
    }

    pub fn get_ref(&self) -> &T
    {
        &self.data
    }

    pub fn get_mut(&mut self) -> &mut T
    {
        self.changed = true;
        &mut self.data
    }

    pub fn get_unmarked_mut(&mut self) -> &mut T
    {
        &mut self.data
    }

    pub fn changed(&self) -> bool
    {
        self.changed
    }

    pub fn consume(&mut self) -> &T
    {
        self.changed = false;

        &self.data
    }

    pub fn consume_change(&mut self) -> bool
    {
        let has_changed = self.changed;
        self.changed = false;

        has_changed
    }

    pub fn consume_borrow(&mut self) -> (&T, bool)
    {
        let has_changed = self.changed;
        self.changed = false;

        (&self.data, has_changed)
    }

    pub fn consume_borrow_mut(&mut self) -> (&mut T, bool)
    {
        let has_changed = self.changed;
        self.changed = false;

        (&mut self.data, has_changed)
    }

    pub fn consume_clone(&mut self) -> (T, bool) where T: Copy
    {
        let has_changed = self.changed;
        self.changed = false;

        (self.data, has_changed)
    }
}