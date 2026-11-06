//! Encoding detection (`UTF-8`, `Windows-1256`) using `chardetng` and `encoding_rs`
//! adhering to Specification Section 4.2 and error code `SF-IMP-002`.

use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};

/// Detects character encoding from raw byte slice, falling back to UTF-8
/// or returning `SF-IMP-002` if encoding confidence is below threshold.
pub fn detect_and_decode(
    bytes: &[u8],
) -> Result<(String, &'static encoding_rs::Encoding), SafiError> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        if let Ok(s) = std::str::from_utf8(&bytes[3..]) {
            return Ok((s.to_owned(), encoding_rs::UTF_8));
        }
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok((s.to_owned(), encoding_rs::UTF_8));
    }

    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);

    let (cow, _, had_errors) = encoding.decode(bytes);
    if had_errors && encoding != encoding_rs::UTF_8 && encoding != encoding_rs::WINDOWS_1256 {
        return Err(SafiError::new(
            ErrorCode::SF_IMP_002,
            ErrorSeverity::Warn,
            "EncodingConfidenceBelowThreshold",
            "import.encoding_unknown",
        ));
    }

    Ok((cow.into_owned(), encoding))
}
