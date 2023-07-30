use sha256::digest;

pub fn get_hash_from_byte_vec(vec: &Vec<u8>) -> String
{
    digest(vec.as_slice())
}