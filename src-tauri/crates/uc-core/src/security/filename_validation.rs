//! Filename validation for secure file transfer.
//!
//! Validates filenames against common attack vectors including path traversal,
//! Windows reserved names, Unicode tricks, and hidden files.
//! Callers must pass the basename only (no path separators).

use std::fmt;

/// Errors returned when a filename fails validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilenameValidationError {
    /// Filename is empty or whitespace-only.
    Empty,
    /// Filename exceeds the 255-byte limit.
    TooLong { len: usize },
    /// Filename contains a null byte.
    NullByte,
    /// Filename contains a control character (0x01..0x1F).
    ControlCharacter { char_code: u8 },
    /// Filename matches a Windows reserved name (e.g., CON, PRN, NUL).
    WindowsReserved { name: String },
    /// Filename starts with a dot (hidden file).
    LeadingDot,
    /// Filename contains a Unicode trick character (RTL override, zero-width, BOM).
    UnicodeTrick { description: &'static str },
    /// Filename contains a path traversal component (`..`).
    PathTraversal,
    /// Filename contains a path separator (`/` or `\`).
    PathSeparator,
}

impl fmt::Display for FilenameValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "filename is empty or whitespace-only"),
            Self::TooLong { len } => {
                write!(f, "filename length {len} exceeds maximum 255 bytes")
            }
            Self::NullByte => write!(f, "filename contains a null byte"),
            Self::ControlCharacter { char_code } => {
                write!(f, "filename contains control character 0x{char_code:02X}")
            }
            Self::WindowsReserved { name } => {
                write!(f, "filename matches Windows reserved name: {name}")
            }
            Self::LeadingDot => write!(f, "filename starts with a dot (hidden file)"),
            Self::UnicodeTrick { description } => {
                write!(f, "filename contains Unicode trick: {description}")
            }
            Self::PathTraversal => {
                write!(f, "filename contains path traversal component (..)")
            }
            Self::PathSeparator => {
                write!(f, "filename contains a path separator (/ or \\)")
            }
        }
    }
}

impl std::error::Error for FilenameValidationError {}

/// Windows reserved device names (case-insensitive).
const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Unicode characters that can be used for filename spoofing attacks.
const UNICODE_TRICKS: &[(char, &str)] = &[
    ('\u{202E}', "RTL override (U+202E)"),
    ('\u{200B}', "zero-width space (U+200B)"),
    ('\u{200C}', "zero-width non-joiner (U+200C)"),
    ('\u{200D}', "zero-width joiner (U+200D)"),
    ('\u{FEFF}', "BOM / zero-width no-break space (U+FEFF)"),
];

/// Validate a filename for safe use in file transfer.
///
/// The input must be a basename (no directory components). Returns `Ok(())`
/// if the filename is safe, or a specific error describing the rejection reason.
pub fn validate_filename(name: &str) -> Result<(), FilenameValidationError> {
    // Empty or whitespace-only
    if name.trim().is_empty() {
        return Err(FilenameValidationError::Empty);
    }

    // Length check (255 bytes)
    if name.len() > 255 {
        return Err(FilenameValidationError::TooLong { len: name.len() });
    }

    // Path separators (must check before path traversal)
    if name.contains('/') || name.contains('\\') {
        return Err(FilenameValidationError::PathSeparator);
    }

    // Path traversal
    if name == ".." || name.contains("..") {
        return Err(FilenameValidationError::PathTraversal);
    }

    // Null bytes
    if name.contains('\0') {
        return Err(FilenameValidationError::NullByte);
    }

    // Control characters (0x01..0x1F)
    for byte in name.bytes() {
        if (0x01..=0x1F).contains(&byte) {
            return Err(FilenameValidationError::ControlCharacter { char_code: byte });
        }
    }

    // Windows reserved names (case-insensitive, with or without extension)
    let stem = name.split('.').next().unwrap_or(name);
    let upper_stem = stem.to_uppercase();
    for reserved in WINDOWS_RESERVED {
        if upper_stem == *reserved {
            return Err(FilenameValidationError::WindowsReserved {
                name: reserved.to_string(),
            });
        }
    }

    // Leading dot (hidden files)
    if name.starts_with('.') {
        return Err(FilenameValidationError::LeadingDot);
    }

    // Unicode tricks
    for (ch, description) in UNICODE_TRICKS {
        if name.contains(*ch) {
            return Err(FilenameValidationError::UnicodeTrick { description });
        }
    }

    Ok(())
}
