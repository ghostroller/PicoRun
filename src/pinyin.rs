//! Compact read-only character dictionary. Conversion happens when an entry is indexed.
//! Character readings use the first dictionary pronunciation; phrase handling is explicit.
const DATA: &[u8] = include_bytes!("../assets/pinyin.bin");
const HEADER_SIZE: usize = 12;
const RECORD_SIZE: usize = 6;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Aliases {
    pub full: String,
    pub initials: String,
}

fn u32_at(offset: usize) -> u32 {
    u32::from_le_bytes(DATA[offset..offset + 4].try_into().unwrap())
}

pub fn reading(character: char) -> Option<&'static str> {
    let count = u32_at(8) as usize;
    let mut low = 0;
    let mut high = count;
    while low < high {
        let mid = low + (high - low) / 2;
        let record = HEADER_SIZE + mid * RECORD_SIZE;
        match u32_at(record).cmp(&(character as u32)) {
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
            std::cmp::Ordering::Equal => {
                let offset = u16::from_le_bytes(DATA[record + 4..record + 6].try_into().unwrap());
                let pool = &DATA[HEADER_SIZE + count * RECORD_SIZE + offset as usize..];
                let length = pool.iter().position(|&byte| byte == 0)?;
                return std::str::from_utf8(&pool[..length]).ok();
            }
        }
    }
    None
}

pub fn aliases(text: &str) -> Aliases {
    // Small, auditable initial phrase overrides. This is not general linguistic disambiguation.
    const PHRASES: &[(&str, &str, &str)] = &[
        ("重庆", "chongqing", "cq"),
        ("重慶", "chongqing", "cq"),
        ("银行", "yinhang", "yh"),
        ("銀行", "yinhang", "yh"),
        ("音乐", "yinyue", "yy"),
        ("音樂", "yinyue", "yy"),
    ];
    let mut result = Aliases::default();
    let mut remaining = text;
    while !remaining.is_empty() {
        if let Some(&(phrase, full, initials)) = PHRASES
            .iter()
            .find(|&&(phrase, _, _)| remaining.starts_with(phrase))
        {
            result.full.push_str(full);
            result.initials.push_str(initials);
            remaining = &remaining[phrase.len()..];
            continue;
        }
        let character = remaining.chars().next().unwrap();
        remaining = &remaining[character.len_utf8()..];
        if let Some(syllable) = reading(character) {
            result.full.push_str(syllable);
            if let Some(initial) = syllable.chars().next() {
                result.initials.push(initial);
            }
        } else {
            // Keep Latin letters, numbers, and unknown characters; don't invent readings.
            for folded in character.to_lowercase() {
                result.full.push(folded);
                result.initials.push(folded);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_dictionary_is_valid() {
        assert_eq!(&DATA[..4], b"PRPY");
        assert_eq!(u32_at(4), 1);
        let count = u32_at(8) as usize;
        assert_eq!(count, 44_435);
        let pool_start = HEADER_SIZE + count * RECORD_SIZE;
        let mut previous = None;
        for i in 0..count {
            let record = HEADER_SIZE + i * RECORD_SIZE;
            let codepoint = u32_at(record);
            assert!(previous.is_none_or(|value| value < codepoint));
            let character = char::from_u32(codepoint).unwrap();
            let offset = u16::from_le_bytes(DATA[record + 4..record + 6].try_into().unwrap());
            assert!(pool_start + (offset as usize) < DATA.len());
            let value = reading(character).unwrap();
            assert!(!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_lowercase()));
            previous = Some(codepoint);
        }
    }

    #[test]
    fn chinese_latin_and_phrase_aliases() {
        for (text, full, initials) in [
            ("微信", "weixin", "wx"),
            ("記事本", "jishiben", "jsb"),
            ("腾讯QQ", "tengxunqq", "txqq"),
            ("网易云音乐", "wangyiyunyinyue", "wyyyy"),
            ("重庆银行", "chongqingyinhang", "cqyh"),
            ("女", "nv", "n"),
        ] {
            assert_eq!(
                aliases(text),
                Aliases {
                    full: full.into(),
                    initials: initials.into()
                }
            );
        }
        assert_eq!(reading('😀'), None);
        assert_eq!(aliases("A😀").full, "a😀");
    }
}
