//! Tiny save blob: puzzle progress and settings. Frontends store it wherever
//! their platform keeps 12 bytes (a file, localStorage, the SD card).

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveData {
    /// Bitmask of solved built-in puzzles.
    pub solved: u32,
    /// Strict mode: ghost preview disabled everywhere.
    pub strict: bool,
    /// Last Adversary difficulty (1–5).
    pub level: u8,
}

impl Default for SaveData {
    fn default() -> Self {
        SaveData {
            solved: 0,
            strict: false,
            level: 2,
        }
    }
}

const MAGIC: &[u8; 4] = b"TPL1";

impl SaveData {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(10);
        v.extend_from_slice(MAGIC);
        v.extend_from_slice(&self.solved.to_le_bytes());
        v.push(self.strict as u8);
        v.push(self.level);
        v
    }

    pub fn from_bytes(b: &[u8]) -> Option<SaveData> {
        if b.len() < 10 || &b[..4] != MAGIC {
            return None;
        }
        Some(SaveData {
            solved: u32::from_le_bytes(b[4..8].try_into().ok()?),
            strict: b[8] != 0,
            level: b[9].clamp(1, 5),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = SaveData {
            solved: 0b1011,
            strict: true,
            level: 4,
        };
        assert_eq!(SaveData::from_bytes(&s.to_bytes()), Some(s));
        assert_eq!(SaveData::from_bytes(b"nope"), None);
    }
}
