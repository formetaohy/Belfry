const GLB_MAGIC: u32 = 0x4654_6C67;
const GLB_VERSION: u32 = 2;
const JSON_CHUNK: u32 = 0x4E4F_534A;
const BINARY_CHUNK: u32 = 0x004E_4942;

pub(crate) struct Container {
    pub(crate) json: Vec<u8>,
    pub(crate) binary: Option<Vec<u8>>,
}

impl Container {
    pub(crate) fn read(bytes: &[u8], what: &str) -> Self {
        if word(bytes, 0, what) == GLB_MAGIC {
            return Self::binary(bytes, what);
        }
        assert!(!bytes.is_empty(), "{what} holds no text");
        Self {
            json: bytes.to_vec(),
            binary: None,
        }
    }

    fn binary(bytes: &[u8], what: &str) -> Self {
        assert!(bytes.len() >= 12, "{what} ends inside its header");
        let version = word(bytes, 4, what);
        assert_eq!(
            version, GLB_VERSION,
            "{what} holds the container version {version}"
        );
        let length = word(bytes, 8, what) as usize;
        assert_eq!(
            length,
            bytes.len(),
            "{what} names {length} bytes and holds {}",
            bytes.len()
        );
        let mut json = None;
        let mut binary = None;
        let mut at = 12;
        while at < bytes.len() {
            let held = word(bytes, at, what) as usize;
            let kind = word(bytes, at + 4, what);
            let start = at + 8;
            let end = start
                .checked_add(held)
                .unwrap_or_else(|| panic!("{what} holds a chunk beyond its bytes"));
            assert!(end <= bytes.len(), "{what} ends inside a chunk");
            match kind {
                JSON_CHUNK => {
                    assert!(
                        json.is_none() && binary.is_none(),
                        "{what} holds a second JSON chunk"
                    );
                    json = Some(bytes[start..end].to_vec());
                }
                BINARY_CHUNK => {
                    assert!(
                        json.is_some(),
                        "{what} holds a binary chunk before its JSON chunk"
                    );
                    assert!(binary.is_none(), "{what} holds a second binary chunk");
                    binary = Some(bytes[start..end].to_vec());
                }
                _ => panic!("{what} holds the unknown chunk {kind}"),
            }
            at = end;
        }
        Self {
            json: json.unwrap_or_else(|| panic!("{what} holds no JSON chunk")),
            binary,
        }
    }
}

fn word(bytes: &[u8], at: usize, what: &str) -> u32 {
    let held = bytes
        .get(at..at + 4)
        .unwrap_or_else(|| panic!("{what} ends inside a word"));
    u32::from_le_bytes([held[0], held[1], held[2], held[3]])
}
