use std::fmt::{self, Display};

#[derive(Clone, Copy, Debug)]
enum Size {
    Bytes(u128),
    Scaled { size: u128, unit: Unit },
}

#[derive(Clone, Copy, Debug)]
enum Unit {
    KiB,
    MiB,
    GiB,
}

const DECIMAL_SCALE: u128 = 10;
const ROUNDING_BASE: u128 = 2;

impl Unit {
    const fn divisor(self) -> u128 {
        match self {
            Self::KiB => 1024,
            Self::MiB => 1024 * 1024,
            Self::GiB => 1024 * 1024 * 1024,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::KiB => "KiB",
            Self::MiB => "MiB",
            Self::GiB => "GiB",
        }
    }
}

impl Display for Size {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytes(size) => write!(formatter, "{size} B"),
            Self::Scaled { unit, .. } => {
                let rounded = self.round();

                write!(
                    formatter,
                    "{}.{:01} {}",
                    rounded / DECIMAL_SCALE,
                    rounded % DECIMAL_SCALE,
                    unit.label(),
                )
            }
        }
    }
}

impl Size {
    fn round(&self) -> u128 {
        match self {
            Self::Bytes(size) => *size,
            Self::Scaled { size, unit } => {
                let divisor = unit.divisor();
                (*size * DECIMAL_SCALE + divisor / ROUNDING_BASE) / divisor
            }
        }
    }
}

pub(super) fn format_size(size: impl Into<u128>) -> impl Display {
    let size = size.into();

    match size {
        size if size >= Unit::GiB.divisor() => Size::Scaled {
            size,
            unit: Unit::GiB,
        },
        size if size >= Unit::MiB.divisor() => Size::Scaled {
            size,
            unit: Unit::MiB,
        },
        size if size >= Unit::KiB.divisor() => Size::Scaled {
            size,
            unit: Unit::KiB,
        },
        size => Size::Bytes(size),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes_and_binary_units() {
        assert_eq!(format_size(0_u64).to_string(), "0 B");
        assert_eq!(format_size(1023_u64).to_string(), "1023 B");
        assert_eq!(format_size(1024_u64).to_string(), "1.0 KiB");
        assert_eq!(format_size(1536_u64).to_string(), "1.5 KiB");
        assert_eq!(format_size(1024_u64 * 1024).to_string(), "1.0 MiB");
        assert_eq!(format_size(1024_u64 * 1024 * 1024).to_string(), "1.0 GiB");
    }

    #[test]
    fn rounds_to_one_decimal_place() {
        assert_eq!(format_size(1280_u64).to_string(), "1.3 KiB");
        assert_eq!(format_size(1228_u64).to_string(), "1.2 KiB");
    }
}
