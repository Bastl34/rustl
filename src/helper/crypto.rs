use xxhash_rust::xxh3::xxh3_128;

pub fn get_hash_from_byte_vec(vec: &Vec<u8>) -> String
{
    let hash = xxh3_128(vec);
    format!("{:032x}", hash)
}