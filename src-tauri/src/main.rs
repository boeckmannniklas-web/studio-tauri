// Unter Windows ohne Konsolenfenster
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    studio_kassenplatz_lib::run();
}
