// Hide the console window in release builds. Dev keeps a console for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if !airport_client_lib::watchdog::run_if_requested() {
        airport_client_lib::run();
    }
}
