#![allow(dead_code)]

#[derive(Debug, Clone)]
pub enum OptionOrId<T>
{
    None,
    Some(T),
    Id(String),
}

impl<T> OptionOrId<T>
{
    pub fn from_id(id: impl Into<String>) -> Self
    {
        Self::Id(id.into())
    }

    pub fn from_id_or_none(option: Option<String>) -> Self
    {
        match option
        {
            Some(value) => Self::Id(value),
            None => Self::None,
        }
    }

    pub fn from_id_vec(id_vec: &Vec<String>) -> Vec<OptionOrId<T>>
    {
        id_vec.iter().map(|id| OptionOrId::from_id(id)).collect()
    }

    pub fn is_some(&self) -> bool
    {
        matches!(self, OptionOrId::Some(_))
    }

    pub fn is_none(&self) -> bool
    {
        matches!(self, OptionOrId::None)
    }

    pub fn is_ref(&self) -> bool
    {
        matches!(self, OptionOrId::Id(_))
    }

    pub fn as_ref(&self) -> Option<&T>
    {
        match self
        {
            OptionOrId::Some(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_mut(&mut self) -> Option<&mut T>
    {
        match self
        {
            OptionOrId::Some(v) => Some(v),
            _ => None,
        }
    }

    pub fn id(&self) -> Option<&str>
    {
        match self
        {
            OptionOrId::Id(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn unwrap(self) -> T
    {
        match self
        {
            OptionOrId::Some(v) => v,
            _ => panic!("called `RefOrValue::unwrap()` on a non-Value variant"),
        }
    }
}

impl<T> Default for OptionOrId<T>
{
    fn default() -> Self
    {
        Self::None
    }
}
