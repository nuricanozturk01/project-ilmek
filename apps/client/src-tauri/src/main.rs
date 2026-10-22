// Release builds on Windows should not open a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ilmek_client_app::run();
}
