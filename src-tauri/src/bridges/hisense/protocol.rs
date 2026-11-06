use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pen {
    pub x: u16,
    pub y: u16,
    pub tip: bool,
    pub barrel: bool,
    pub valid: bool,
}
pub fn pen(bytes: &[u8]) -> Option<Pen> {
    if bytes.len() < 10 || bytes[0] != 0x3f {
        return None;
    }
    let x = u16::from_le_bytes([bytes[2], bytes[3]]);
    let y = u16::from_le_bytes([bytes[4], bytes[5]]);
    Some(Pen {
        x,
        y,
        tip: bytes[1] & 1 != 0,
        barrel: bytes[1] & 2 != 0,
        valid: bytes[1] & 0x20 != 0 && x <= 21240 && y <= 15980,
    })
}
pub fn keyboard(bytes: &[u8]) -> Option<BTreeSet<String>> {
    if bytes.first() != Some(&1) || bytes.len() < 9 {
        return None;
    }
    if bytes[3..9].iter().any(|k| (1..=3).contains(k)) {
        return None;
    }
    let mut sources: BTreeSet<_> = bytes[3..9]
        .iter()
        .filter(|k| **k != 0)
        .map(|k| format!("key:{k:02x}"))
        .collect();
    for bit in 0..8 {
        if bytes[1] & (1 << bit) != 0 {
            sources.insert(format!("key:{:02x}", 0xe0 + bit));
        }
    }
    Some(sources)
}
pub fn valid_source(s: &str) -> bool {
    if matches!(s, "pen:tip" | "pen:barrel") {
        return true;
    }
    if let Some(key) = s.strip_prefix("key:") {
        return key.len() == 2 && u8::from_str_radix(key, 16).is_ok();
    }
    if let Some(raw) = s.strip_prefix("raw:") {
        return raw.len() >= 4
            && raw.len() <= 64
            && raw.len() % 2 == 0
            && raw.bytes().all(|b| b.is_ascii_hexdigit())
            && !raw.starts_with("3f");
    }
    false
}
pub fn raw_source(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2
        || bytes.len() > 32
        || matches!(bytes[0], 1 | 0x3f)
        || bytes[1..].iter().all(|b| *b == 0)
    {
        return None;
    }
    let end = bytes.iter().rposition(|b| *b != 0)? + 1;
    let source = format!(
        "raw:{}",
        bytes[..end]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    valid_source(&source).then_some(source)
}
pub fn usage_vk(source: &str) -> Option<u16> {
    let u = u8::from_str_radix(source.strip_prefix("key:")?, 16).ok()?;
    Some(match u {
        4..=0x1d => 0x41 + (u - 4) as u16,
        0x1e..=0x26 => 0x31 + (u - 0x1e) as u16,
        0x3a..=0x45 => 0x70 + (u - 0x3a) as u16,
        0x59..=0x61 => 0x61 + (u - 0x59) as u16,
        0x27 => 0x30,
        0x28 | 0x58 => 0xd,
        0x29 => 0x1b,
        0x2a => 8,
        0x2b => 9,
        0x2c => 0x20,
        0x4a => 0x24,
        0x4b => 0x21,
        0x4c => 0x2e,
        0x4d => 0x23,
        0x4e => 0x22,
        0x4f => 0x27,
        0x50 => 0x25,
        0x51 => 0x28,
        0x52 => 0x26,
        0x53 => 0x90,
        0x54 => 0x6f,
        0x55 => 0x6a,
        0x56 => 0x6d,
        0x57 => 0x6b,
        0x62 => 0x60,
        0x63 => 0x6e,
        0x65 => 0x5d,
        0x7f => 0xad,
        0x80 => 0xaf,
        0x81 => 0xae,
        0xe0 => 0xa2,
        0xe1 => 0xa0,
        0xe2 => 0xa4,
        0xe3 => 0x5b,
        0xe4 => 0xa3,
        0xe5 => 0xa1,
        0xe6 => 0xa5,
        0xe7 => 0x5c,
        _ => return None,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_reports_decode() {
        let p = pen(&[0x3f, 0x21, 0x84, 0x1a, 0xdf, 0x26, 0, 0, 0, 0]).unwrap();
        assert_eq!((p.x, p.y, p.tip), (6788, 9951, true));
        assert!(p.valid);
        assert_eq!(
            keyboard(&[1, 0, 0, 0x4f, 0, 0, 0, 0, 0])
                .unwrap()
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["key:4f"]
        );
        assert_eq!(usage_vk("key:4f"), Some(0x27));
    }
    #[test]
    fn rollover_does_not_release_held_keys() {
        assert!(keyboard(&[1, 0, 0, 1, 1, 1, 1, 1, 1]).is_none());
        assert!(pen(&[0x3f]).is_none());
        assert!(
            !pen(&[0x3f, 0x20, 0xff, 0xff, 0, 0, 0, 0, 0, 0])
                .unwrap()
                .valid
        );
        assert!(raw_source(&[0x3f, 0x20, 1, 2]).is_none());
    }
}
