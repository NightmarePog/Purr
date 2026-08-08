use std::{
    fmt::{Display, Write as _},
    fs,
    path::{Path, PathBuf},
};

use const_format::{Case, map_ascii_case, str_replace};

use alpm::{Alpm, LoadedPackage, SigLevel};

use crate::build::BuildError;

pub struct Database<'a>(&'a Path);

pub struct Artifact<'a>(&'a Path);

impl<'a> Database<'a> {
    pub const fn new(path: &'a Path) -> Self {
        Self(path)
    }

    pub fn push(&self, artifact: Artifact<'_>) -> Result<(), BuildError> {
        artifact.load(self, |package| self.register(package))
    }

    fn register(&self, package: &LoadedPackage<'_>) -> Result<(), BuildError> {
        let package_directory = self.package_directory(&package);

        fs::create_dir_all(&package_directory)?;
        PackageDescription::from(package).write_description_file(&package_directory)?;
        self.write_package_file_list(&package_directory, package)
    }

    fn package_directory(&self, package: &LoadedPackage<'_>) -> PathBuf {
        self.0
            .join("local")
            .join(format!("{}-{}", package.name(), package.version()))
    }

    fn write_package_file_list(
        &self,
        package_directory: &Path,
        package: &LoadedPackage<'_>,
    ) -> Result<(), BuildError> {
        let mut file_list = String::from("%FILES%\n");

        package.files().files().iter().for_each(|file| {
            let file_name = String::from_utf8_lossy(file.name());
            if is_package_file_entry(&file_name) {
                file_list.push_str(file_name.trim_start_matches("./"));
                file_list.push('\n');
            }
        });

        file_list.push('\n');
        fs::write(package_directory.join("files"), file_list)?;
        Ok(())
    }
}

impl<'a> Artifact<'a> {
    pub const fn new(path: &'a Path) -> Self {
        Self(path)
    }
}

impl Artifact<'_> {
    fn load<T>(
        &self,
        database: &Database<'_>,
        process: impl for<'b> FnOnce(&LoadedPackage<'b>) -> Result<T, BuildError>,
    ) -> Result<T, BuildError> {
        let alpm = Alpm::new("/".to_owned(), database.0.to_string_lossy().into_owned())?;
        let package = alpm.pkg_load(self.0.to_string_lossy().into_owned(), true, SigLevel::NONE)?;
        process(&package)
    }
}

fn is_package_file_entry(file_name: &str) -> bool {
    !file_name.ends_with('/')
        && (!file_name.starts_with('.') || file_name.starts_with("./"))
        && !matches!(file_name, ".BUILDINFO" | ".MTREE" | ".PKGINFO" | ".INSTALL")
}

struct PackageDescription {
    content: String,
}

const ALLOWED_FIELD_NAMES: [&'static str; 16] = [
    "NAME",
    "VERSION",
    "BASE",
    "DESC",
    "URL",
    "ARCH",
    "BUILDDATE",
    "INSTALLDATE",
    "PACKAGER",
    "SIZE",
    "REASON",
    "LICENSE",
    "VALIDATION",
    "DEPENDS",
    "OPTDEPENDS",
    "PROVIDES",
];

impl PackageDescription {
    fn new() -> Self {
        Self {
            content: String::with_capacity(1024),
        }
    }

    fn write_field<T: Display>(
        &mut self,
        name: &str,
        values: impl IntoIterator<Item = T>,
    ) -> &mut Self {
        debug_assert!(ALLOWED_FIELD_NAMES.contains(&name));
        let _ = writeln!(self.content, "%{name}%");
        values.into_iter().for_each(|value| {
            let _ = writeln!(self.content, "{value}");
        });
        self.content.push('\n');
        self
    }

    fn write_description_file(self, package_directory: &Path) -> Result<(), BuildError> {
        fs::write(package_directory.join("desc"), self.content)?;
        Ok(())
    }
}

macro_rules! write_fields_new_fmtname {
    ($name:ident) => {
        str_replace!(map_ascii_case!(Case::Upper, stringify!($name)), '_', "")
    };
}

macro_rules! write_fields_new_impl {
    ($from:ident $self:ident) => {};
    ($from:ident $self:ident $name:ident($($nameargs:tt)*)$(.$methodname:ident($($methodargs:tt)*))*, $($rest:tt)*) => {
        $self.write_field(write_fields_new_fmtname!($name), [$from.$name($($nameargs)*)$(.$methodname($($methodargs)*))*]);
        write_fields_new_impl!($from $self $($rest)*);
    };
    ($from:ident $self:ident ..$name:ident($($nameargs:tt)*)$(.$methodname:ident($($methodargs:tt)*))*, $($rest:tt)*) => {
        $self.write_field(write_fields_new_fmtname!($name), $from.$name($($nameargs)*)$(.$methodname($($methodargs)*))*);
        write_fields_new_impl!($from $self $($rest)*);
    };
    ($from:ident $self:ident $name:ident => $value:expr, $($rest:tt)*) => {
        $self.write_field(write_fields_new_fmtname!($name), [$value]);
        write_fields_new_impl!($from $self $($rest)*);
    };
    ($from:ident $self:ident ..$name:ident => $value:expr, $($rest:tt)*) => {
        $self.write_field(write_fields_new_fmtname!($name), $value);
        write_fields_new_impl!($from $self $($rest)*);
    };
}

macro_rules! write_fields_new {
    (from = $from:ident, {
        $($fields:tt)+
    }) => {{
        let mut s = Self::new();
        write_fields_new_impl!($from s $($fields)+);
        s
    }};
}

impl<'a> From<&LoadedPackage<'a>> for PackageDescription {
    fn from(package: &LoadedPackage<'a>) -> Self {
        write_fields_new!(from = package, {
            name(),
            version(),
            base().unwrap_or(package.name()),
            desc().unwrap_or_default(),
            url().unwrap_or_default(),
            arch().unwrap_or_default(),
            build_date(),
            installdate => package.build_date(),
            packager().unwrap_or_default(),
            size(),
            reason => 0,
            ..license => package.licenses(),
            validation => "none",
            ..depends(),
            ..optdepends(),
            ..provides(),
        })
    }
}
