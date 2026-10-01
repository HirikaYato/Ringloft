// В релизе под Windows не поднимаем консольное окно рядом с приложением.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ringloft_lib::run();
}
