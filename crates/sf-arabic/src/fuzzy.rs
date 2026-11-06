//! Arabic Phonetic Key and Jaro-Winkler Fuzzy Matching (Section 4.4, 4.6)

pub fn arabic_phonetic_key(input: &str) -> String {
    let normalized = input
        .chars()
        .map(|c| match c {
            'أ' | 'إ' | 'آ' => 'ا',
            'ة' => 'ه',
            'ى' | 'ي' => 'ي',
            'ؤ' => 'و',
            'ئ' => 'ي',
            'گ' => 'ج',
            'ڤ' => 'ف',
            'پ' => 'ب',
            _ => c,
        })
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
        .collect::<String>();

    // Simple phonetic reduction (e.g., remove common diacritics / elongation)
    normalized.chars().filter(|&c| c != 'ـ' && c != 'ً' && c != 'ٌ' && c != 'ٍ' && c != 'َ' && c != 'ُ' && c != 'ِ' && c != 'ّ' && c != 'ْ').collect()
}

pub fn jaro_winkler_similarity(s1: &str, s2: &str) -> f64 {
    if s1 == s2 {
        return 1.0;
    }
    let chars1: Vec<char> = s1.chars().collect();
    let chars2: Vec<char> = s2.chars().collect();
    let len1 = chars1.len();
    let len2 = chars2.len();
    if len1 == 0 || len2 == 0 {
        return 0.0;
    }

    let match_distance = (std::cmp::max(len1, len2) / 2) - 1;
    let mut s1_matches = vec![false; len1];
    let mut s2_matches = vec![false; len2];

    let mut matches = 0;
    let mut transpositions = 0;

    for i in 0..len1 {
        let start = if i > match_distance { i - match_distance } else { 0 };
        let end = std::cmp::min(i + match_distance + 1, len2);
        for j in start..end {
            if !s2_matches[j] && chars1[i] == chars2[j] {
                s1_matches[i] = true;
                s2_matches[j] = true;
                matches += 1;
                break;
            }
        }
    }

    if matches == 0 {
        return 0.0;
    }

    let mut k = 0;
    for i in 0..len1 {
        if s1_matches[i] {
            while !s2_matches[k] {
                k += 1;
            }
            if chars1[i] != chars2[k] {
                transpositions += 1;
            }
            k += 1;
        }
    }

    let m = matches as f64;
    let jaro = (m / len1 as f64 + m / len2 as f64 + (m - (transpositions as f64 / 2.0)) / m) / 3.0;
    let mut prefix = 0;
    for i in 0..std::cmp::min(4, std::cmp::min(len1, len2)) {
        if chars1[i] == chars2[i] {
            prefix += 1;
        } else {
            break;
        }
    }

    jaro + (prefix as f64 * 0.1 * (1.0 - jaro))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phonetic_normalization() {
        let key1 = arabic_phonetic_key("أحمد");
        let key2 = arabic_phonetic_key("احمد");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_jaro_winkler() {
        let score = jaro_winkler_similarity("محمد", "محمود");
        assert!(score > 0.7);
    }
}
