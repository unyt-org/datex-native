use std::path::PathBuf;

pub fn get_datex_base_dir() -> Option<PathBuf> {
    home::home_dir().map(|mut path| {
        path.push(".datex");
        path
    })
}