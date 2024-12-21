use zcash_warp::{
    db::account_manager::parse_seed_phrase, network::Network::Main, utils::ua::decode_address,
};

#[tauri::command(rename_all = "snake_case")]
pub fn is_valid_seed(seed: String) -> bool {
    parse_seed_phrase(&seed).is_ok()
}

#[tauri::command(rename_all = "snake_case")]
pub fn is_valid_address(address: String) -> bool {
    decode_address(&Main, &address).is_ok()
}
