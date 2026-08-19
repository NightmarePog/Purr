pub const AUR_URL: &str = "https://aur.archlinux.org";

pub fn build_path() -> Option<std::path::PathBuf> {
    dirs::cache_dir().map(|directory| directory.join("aur-pkg-manager/build"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn build_cache_is_not_relative_to_the_working_directory() {
        let path = super::build_path().expect("user cache directory");

        assert!(path.is_absolute());
        assert!(path.ends_with("aur-pkg-manager/build"));
    }
}
