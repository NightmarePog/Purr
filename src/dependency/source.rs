#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageSource {
    Installed,
    Repo,
    Aur,
}
