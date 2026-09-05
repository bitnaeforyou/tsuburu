//! B-tree 노드 디코딩.
//!
//! 노드는 고정 464바이트이고 모든 정수는 빅엔디언이다. 포맷은
//! `docs/superpowers/specs/2026-09-05-tsuburu-design.md` 부록 A 참고.
//! 이 모듈은 바이트를 구조로 바꾸는 일만 한다. 탐색은 `index`가 맡는다.

use crate::B;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("buffer too short at offset {0}")]
    Truncated(usize),
    #[error("invalid key size {0}")]
    KeySize(i32),
    #[error("invalid entry count {0}")]
    Count(i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub keys: Vec<Vec<u8>>,
    pub datas: Vec<(u64, i32)>,
    pub subnode_addresses: [u64; B + 1],
}

impl Node {
    pub fn is_leaf(&self) -> bool {
        self.subnode_addresses.iter().all(|&a| a == 0)
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Truncated(self.pos))?;
        let slice = self.data.get(self.pos..end).ok_or(DecodeError::Truncated(self.pos))?;
        self.pos = end;
        Ok(slice)
    }

    fn i32(&mut self) -> Result<i32, DecodeError> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
}

pub fn decode_node(data: &[u8]) -> Result<Node, DecodeError> {
    let mut c = Cursor { data, pos: 0 };

    let n_keys = c.i32()?;
    if !(0..=B as i32).contains(&n_keys) {
        return Err(DecodeError::Count(n_keys));
    }
    let mut keys = Vec::with_capacity(n_keys as usize);
    for _ in 0..n_keys {
        let size = c.i32()?;
        if size <= 0 || size > 32 {
            return Err(DecodeError::KeySize(size));
        }
        keys.push(c.take(size as usize)?.to_vec());
    }

    let n_datas = c.i32()?;
    if !(0..=B as i32).contains(&n_datas) {
        return Err(DecodeError::Count(n_datas));
    }
    let mut datas = Vec::with_capacity(n_datas as usize);
    for _ in 0..n_datas {
        let offset = c.u64()?;
        let length = c.i32()?;
        datas.push((offset, length));
    }

    let mut subnode_addresses = [0u64; B + 1];
    for slot in subnode_addresses.iter_mut() {
        *slot = c.u64()?;
    }

    Ok(Node { keys, datas, subnode_addresses })
}

#[cfg(test)]
pub(crate) fn write_node(out: &mut [u8], keys: &[&[u8]], datas: &[(u64, i32)], subs: &[u64]) {
    let mut pos = 0usize;
    let mut put = |bytes: &[u8], pos: &mut usize| {
        out[*pos..*pos + bytes.len()].copy_from_slice(bytes);
        *pos += bytes.len();
    };
    put(&(keys.len() as i32).to_be_bytes(), &mut pos);
    for k in keys {
        put(&(k.len() as i32).to_be_bytes(), &mut pos);
        put(k, &mut pos);
    }
    put(&(datas.len() as i32).to_be_bytes(), &mut pos);
    for (offset, length) in datas {
        put(&offset.to_be_bytes(), &mut pos);
        put(&length.to_be_bytes(), &mut pos);
    }
    for i in 0..(B + 1) {
        put(&subs.get(i).copied().unwrap_or(0).to_be_bytes(), &mut pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_real_root_node() {
        let data = include_bytes!("../tests/fixtures/root_node.bin");
        assert_eq!(data.len(), crate::MAX_NODE_SIZE as usize);
        let node = decode_node(data).unwrap();
        assert!(!node.keys.is_empty());
        assert!(node.keys.iter().all(|k| !k.is_empty() && k.len() <= 32));
        assert_eq!(node.keys.len(), node.datas.len());
        assert!(!node.is_leaf(), "root of a large index must have children");
    }

    #[test]
    fn rejects_absurd_key_size() {
        let mut data = vec![0u8; 464];
        data[0..4].copy_from_slice(&1i32.to_be_bytes());
        data[4..8].copy_from_slice(&99i32.to_be_bytes());
        assert_eq!(decode_node(&data), Err(DecodeError::KeySize(99)));
    }

    #[test]
    fn rejects_negative_key_count() {
        let mut data = vec![0u8; 464];
        data[0..4].copy_from_slice(&(-1i32).to_be_bytes());
        assert_eq!(decode_node(&data), Err(DecodeError::Count(-1)));
    }

    #[test]
    fn rejects_truncated_buffer() {
        assert!(decode_node(&[0u8; 3]).is_err());
    }

    #[test]
    fn round_trips_a_synthetic_node() {
        let mut buf = vec![0u8; 464];
        write_node(&mut buf, &[&[1, 2], &[3]], &[(9, 4), (7, 8)], &[464]);
        let node = decode_node(&buf).unwrap();
        assert_eq!(node.keys, vec![vec![1, 2], vec![3]]);
        assert_eq!(node.datas, vec![(9, 4), (7, 8)]);
        assert_eq!(node.subnode_addresses[0], 464);
        assert!(!node.is_leaf());
    }

    #[test]
    fn node_with_no_children_is_leaf() {
        let mut buf = vec![0u8; 464];
        write_node(&mut buf, &[&[1]], &[(0, 1)], &[]);
        assert!(decode_node(&buf).unwrap().is_leaf());
    }
}
