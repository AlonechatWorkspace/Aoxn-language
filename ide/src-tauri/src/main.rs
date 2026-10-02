// A GUI app: no console window behind the workbench. `windows_subsystem`
// is set only for release builds so `println!` still reaches a terminal
// while developing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    aoxn_ide_lib::run()
}