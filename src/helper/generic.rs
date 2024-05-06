use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub fn get_millis() -> u64
{
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

pub fn get_secs() -> u64
{
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u64
}


pub fn match_by_include_exclude(str: &String, include: &Vec<String>, exclude: &Vec<String>) -> bool
{
    for inc in include
    {
        if str.find(inc).is_none()
        {
            return false;
        }
    }

    for ex in exclude
    {
        if str.find(ex).is_some()
        {
            return false;
        }
    }

    true
}