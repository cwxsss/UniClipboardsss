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

#[cfg(test)]
mod tests {
    use super::*;

    // === Positive cases (should pass) ===

    #[test]
    fn valid_simple_filename() {
        assert!(validate_filename("report.pdf").is_ok());
    }

    #[test]
    fn valid_filename_with_spaces() {
        assert!(validate_filename("my document.txt").is_ok());
    }

    #[test]
    fn valid_filename_with_unicode() {
        assert!(validate_filename("resume_2024.pdf").is_ok());
    }

    #[test]
    fn valid_filename_with_cjk() {
        assert!(validate_filename("document.txt").is_ok());
    }

    #[test]
    fn valid_filename_max_length() {
        let name = "a".repeat(255);
        assert!(validate_filename(&name).is_ok());
    }

    #[test]
    fn valid_filename_with_multiple_dots() {
        assert!(validate_filename("archive.tar.gz").is_ok());
    }

    // === Empty / whitespace ===

    #[test]
    fn reject_empty() {
        assert_eq!(validate_filename(""), Err(FilenameValidationError::Empty));
    }

    #[test]
    fn reject_whitespace_only() {
        assert_eq!(validate_filename("   "), Err(FilenameValidationError::Empty));
    }

    #[test]
    fn reject_tab_only() {
        // Tab is also a control character but empty check comes first for whitespace
        let result = validate_filename("\t");
        assert!(result.is_err());
    }

    // === Length ===

    #[test]
    fn reject_too_long() {
        let name = "b".repeat(256);
        assert_eq!(
            validate_filename(&name),
            Err(FilenameValidationError::TooLong { len: 256 })
        );
    }

    // === Null bytes ===

    #[test]
    fn reject_null_byte() {
        assert_eq!(
            validate_filename("file\0name"),
            Err(FilenameValidationError::NullByte)
        );
    }

    // === Control characters ===

    #[test]
    fn reject_control_char_bel() {
        assert_eq!(
            validate_filename("file\x07name"),
            Err(FilenameValidationError::ControlCharacter { char_code: 0x07 })
        );
    }

    #[test]
    fn reject_control_char_soh() {
        assert_eq!(
            validate_filename("\x01start"),
            Err(FilenameValidationError::ControlCharacter { char_code: 0x01 })
        );
    }

    #[test]
    fn reject_control_char_us() {
        assert_eq!(
            validate_filename("end\x1F"),
            Err(FilenameValidationError::ControlCharacter { char_code: 0x1F })
        );
    }

    // === Windows reserved names ===

    #[test]
    fn reject_con() {
        assert_eq!(
            validate_filename("CON"),
            Err(FilenameValidationError::WindowsReserved {
                name: "CON".to_string()
            })
        );
    }

    #[test]
    fn reject_con_case_insensitive() {
        assert_eq!(
            validate_filename("con"),
            Err(FilenameValidationError::WindowsReserved {
                name: "CON".to_string()
            })
        );
    }

    #[test]
    fn reject_con_with_extension() {
        assert_eq!(
            validate_filename("CON.txt"),
            Err(FilenameValidationError::WindowsReserved {
                name: "CON".to_string()
            })
        );
    }

    #[test]
    fn reject_prn() {
        assert_eq!(
            validate_filename("PRN"),
            Err(FilenameValidationError::WindowsReserved {
                name: "PRN".to_string()
            })
        );
    }

    #[test]
    fn reject_nul() {
        assert_eq!(
            validate_filename("NUL"),
            Err(FilenameValidationError::WindowsReserved {
                name: "NUL".to_string()
            })
        );
    }

    #[test]
    fn reject_com1() {
        assert_eq!(
            validate_filename("COM1"),
            Err(FilenameValidationError::WindowsReserved {
                name: "COM1".to_string()
            })
        );
    }

    #[test]
    fn reject_lpt9() {
        assert_eq!(
            validate_filename("lpt9"),
            Err(FilenameValidationError::WindowsReserved {
                name: "LPT9".to_string()
            })
        );
    }

    // Not reserved: "CONX" is fine
    #[test]
    fn allow_con_prefix_not_exact() {
        assert!(validate_filename("CONX").is_ok());
    }

    // === Leading dot ===

    #[test]
    fn reject_leading_dot() {
        assert_eq!(
            validate_filename(".hidden"),
            Err(FilenameValidationError::LeadingDot)
        );
    }

    #[test]
    fn reject_dotdot_as_hidden_or_traversal() {
        // ".." triggers path traversal
        let result = validate_filename("..");
        assert!(result.is_err());
    }

    // === Unicode tricks ===

    #[test]
    fn reject_rtl_override() {
        assert_eq!(
            validate_filename("file\u{202E}txt.exe"),
            Err(FilenameValidationError::UnicodeTrick {
                description: "RTL override (U+202E)"
            })
        );
    }

    #[test]
    fn reject_zero_width_space() {
        assert_eq!(
            validate_filename("file\u{200B}name"),
            Err(FilenameValidationError::UnicodeTrick {
                description: "zero-width space (U+200B)"
            })
        );
    }

    #[test]
    fn reject_zero_width_non_joiner() {
        assert_eq!(
            validate_filename("file\u{200C}name"),
            Err(FilenameValidationError::UnicodeTrick {
                description: "zero-width non-joiner (U+200C)"
            })
        );
    }

    #[test]
    fn reject_zero_width_joiner() {
        assert_eq!(
            validate_filename("file\u{200D}name"),
            Err(FilenameValidationError::UnicodeTrick {
                description: "zero-width joiner (U+200D)"
            })
        );
    }

    #[test]
    fn reject_bom() {
        assert_eq!(
            validate_filename("\u{FEFF}file.txt"),
            Err(FilenameValidationError::UnicodeTrick {
                description: "BOM / zero-width no-break space (U+FEFF)"
            })
        );
    }

    // === Path traversal ===

    #[test]
    fn reject_dotdot() {
        assert_eq!(
            validate_filename(".."),
            Err(FilenameValidationError::PathTraversal)
        );
    }

    #[test]
    fn reject_dotdot_embedded() {
        // "a..b" contains ".." substring
        assert_eq!(
            validate_filename("a..b"),
            Err(FilenameValidationError::PathTraversal)
        );
    }

    // === Path separators ===

    #[test]
    fn reject_forward_slash() {
        assert_eq!(
            validate_filename("dir/file.txt"),
            Err(FilenameValidationError::PathSeparator)
        );
    }

    #[test]
    fn reject_backslash() {
        assert_eq!(
            validate_filename("dir\\file.txt"),
            Err(FilenameValidationError::PathSeparator)
        );
    }

    #[test]
    fn reject_traversal_with_separator() {
        // Path separator check triggers before traversal check
        assert_eq!(
            validate_filename("../etc/passwd"),
            Err(FilenameValidationError::PathSeparator)
        );
    }

    // === Display trait ===

    #[test]
    fn display_messages_are_descriptive() {
        assert!(FilenameValidationError::Empty.to_string().contains("empty"));
        assert!(FilenameValidationError::TooLong { len: 300 }
            .to_string()
            .contains("300"));
        assert!(FilenameValidationError::NullByte
            .to_string()
            .contains("null"));
        assert!(FilenameValidationError::PathSeparator
            .to_string()
            .contains("separator"));
    }
}
