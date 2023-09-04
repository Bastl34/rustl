use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub fn get_millis() -> u64
{
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}