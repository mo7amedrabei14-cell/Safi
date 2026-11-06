//! Arabic text normalizer implementing Specification Section 4.4.

/// Options controlling Arabic text normalization behaviors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormOptions {
    pub unify_alif: bool,
    pub unify_ya: bool,
    pub unify_ha: bool,
    pub remove_tatweel: bool,
    pub remove_diacritics: bool,
    pub clean_whitespace: bool,
    pub normalize_digits: bool, // true = Eastern/Indic to Western (0-9)
    pub join_compound_names: bool,
    pub remove_titles: bool,
}

impl Default for NormOptions {
    fn default() -> Self {
        Self {
            unify_alif: true,
            unify_ya: true,
            unify_ha: true,
            remove_tatweel: true,
            remove_diacritics: true,
            clean_whitespace: true,
            normalize_digits: true,
            join_compound_names: true,
            remove_titles: false, // Optional by default
        }
    }
}

/// Helper struct wrapping normalization rules and options.
#[derive(Debug, Clone, Copy, Default)]
pub struct ArabicNormalizer {
    pub options: NormOptions,
}

impl ArabicNormalizer {
    pub const fn new(options: NormOptions) -> Self {
        Self { options }
    }

    pub fn normalize(&self, input: &str) -> String {
        normalize_text(input, self.options)
    }
}

/// Normalizes an Arabic string based on the given options.
pub fn normalize_text(input: &str, opts: NormOptions) -> String {
    if input.is_empty() {
        return String::new();
    }

    // Step 1: Remove titles if requested
    let mut working = input.to_string();
    if opts.remove_titles {
        let titles = ["د.", "م.", "أ.", "الأستاذ", "الاستاذ", "مهندس", "دكتور"];
        for title in &titles {
            if working.starts_with(title) {
                working = working[title.len()..].trim_start().to_string();
            }
        }
    }

    // Iterate by character / grapheme / char
    let mut chars: Vec<char> = working.chars().collect();

    // Step 2: Remove diacritics and tatweel
    let mut filtered = Vec::with_capacity(chars.len());
    for &c in &chars {
        if opts.remove_diacritics && (0x064B..=0x065F).contains(&(c as u32)) || c == '\u{0670}' {
            continue;
        }
        if opts.remove_tatweel && c == '\u{0640}' {
            continue;
        }
        filtered.push(c);
    }
    chars = filtered;

    // Step 3: Character-level normalization (Alif, Ya, Ha, Digits)
    let mut normalized_chars = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        // Unify Alif: أ, إ, آ, ٱ -> ا
        let mapped_c = if opts.unify_alif {
            match c {
                'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
                _ => c,
            }
        } else {
            c
        };

        // Unify Ya: ى -> ي
        let mapped_c = if opts.unify_ya {
            match mapped_c {
                'ى' => 'ي',
                _ => mapped_c,
            }
        } else {
            mapped_c
        };

        // Unify Ha/Ta Marbuta: ة -> ه (at end or generally for matching)
        let mapped_c = if opts.unify_ha {
            match mapped_c {
                'ة' => 'ه',
                _ => mapped_c,
            }
        } else {
            mapped_c
        };

        // Normalize digits: Eastern Arabic (٠-٩: U+0660..U+0669) and Extended Arabic-Indic (۰-۹: U+06F0..U+06F9) -> Western (0-9)
        let mapped_c = if opts.normalize_digits {
            match mapped_c {
                '٠'..='٩' => ((mapped_c as u32 - '٠' as u32) + '0' as u32) as u8 as char,
                '۰'..='۹' => ((mapped_c as u32 - '۰' as u32) + '0' as u32) as u8 as char,
                '٫' => '.',
                '٬' => ',',
                '؟' => '?',
                _ => mapped_c,
            }
        } else {
            match mapped_c {
                '٫' => '.',
                '٬' => ',',
                '؟' => '?',
                _ => mapped_c,
            }
        };

        normalized_chars.push(mapped_c);
        i += 1;
    }

    // Step 4: Join compound names ("عبد", "أبو", "أم" + space + next word)
    let s: String = normalized_chars.into_iter().collect();
    let mut final_s = s;
    if opts.join_compound_names {
        let compounds = ["عبد ", "أبو ", "ابو ", "أم ", "ام "];
        for comp in &compounds {
            let mut updated = String::new();
            let mut search_from = 0;
            while let Some(idx) = final_s[search_from..].find(comp) {
                let absolute_idx = search_from + idx;
                updated.push_str(&final_s[search_from..absolute_idx]);
                // Remove the space after compound word
                let trimmed_comp = comp.trim_end();
                updated.push_str(trimmed_comp);
                search_from = absolute_idx + comp.len();
            }
            updated.push_str(&final_s[search_from..]);
            final_s = updated;
        }
    }

    // Step 5: Clean whitespace and invisible characters (ZWJ, ZWNJ, RLM, LRM, NBSP)
    if opts.clean_whitespace {
        let mut cleaned = String::with_capacity(final_s.len());
        let mut last_was_space = true; // trim leading
        for c in final_s.chars() {
            // Invisible / control chars
            if c == '\u{200C}'
                || c == '\u{200D}'
                || c == '\u{200E}'
                || c == '\u{200F}'
                || c == '\u{202A}'
                || c == '\u{202B}'
                || c == '\u{202C}'
                || c == '\u{202D}'
                || c == '\u{202E}'
                || c == '\u{FEFF}'
                || c == '\u{00AD}'
            {
                continue;
            }
            if c.is_whitespace() || c == '\u{00A0}' {
                if !last_was_space {
                    cleaned.push(' ');
                    last_was_space = true;
                }
            } else {
                cleaned.push(c);
                last_was_space = false;
            }
        }
        // Trim trailing space
        if cleaned.ends_with(' ') {
            cleaned.pop();
        }
        final_s = cleaned;
    }

    final_s
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_basic_normalization() {
        let normalizer = ArabicNormalizer::default();

        // Alif unification, diacritics removal, tatweel, ta marbuta, digits
        let raw = "أَحْمَدُ مُحَمَّدْ  الْـعَبْدِي ـــ ١٢٣٤٥";
        let norm = normalizer.normalize(raw);
        assert_eq!(norm, "احمد محمد العبدي 12345");
    }

    #[test]
    fn test_compound_names_joining() {
        let normalizer = ArabicNormalizer::default();
        let raw = "عبد الله و أبو بكر";
        let norm = normalizer.normalize(raw);
        // Alif unification (Step 1) turns "أبو" into "ابو" before compound joining.
        assert_eq!(norm, "عبدالله و ابوبكر");
    }

    proptest! {
        #[test]
        fn prop_normalization_idempotent(s in "\\PC*") {
            let normalizer = ArabicNormalizer::default();
            let once = normalizer.normalize(&s);
            let twice = normalizer.normalize(&once);
            prop_assert_eq!(once, twice);
        }
    }
}
