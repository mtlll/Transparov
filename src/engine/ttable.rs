use std::cmp;
use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::sync::atomic::{AtomicU64, AtomicU32, Ordering, AtomicU8};
use std::mem;

use chess::{Board, ChessMove, Square, ALL_PIECES};

use crate::engine::eval;
use std::time::{Instant, Duration};
use std::iter;
use std::mem::MaybeUninit;
use std::num::Wrapping;
use crate::engine::eval::Eval;

pub type CacheTable = Arc<TTable>;

#[derive(Clone, Copy, Ord, PartialOrd, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct Move16 {
    mv: u16
}

impl From<u16> for Move16 {
    fn from(mv: u16) -> Self {
        Move16 {
            mv
        }
    }
}

impl From<Move16> for u16 {
    fn from(mv: Move16) -> Self {
        mv.mv
    }
}

impl From<ChessMove> for Move16 {
    fn from(mv: ChessMove) -> Self {
        let src = (mv.get_source().to_index() << 10) as u16;
        let dest = (mv.get_dest().to_index() << 4) as u16;
        let mut prom_flag: u16 = 0;

        let prom_piece = mv.get_promotion().map_or(0, |piece| {
            prom_flag = 1;
            ((piece.to_index() as u16 - 1) & 3) as u16
        });

        Move16::from(src | dest | (prom_flag << 2) | prom_piece)
    }
}

impl From<Move16> for ChessMove {
    fn from(mv: Move16) -> Self {
        let mv16: u16 = mv.into();
        let src = unsafe {
            let idx = (mv16 >> 10) & 63;
            debug_assert!(idx < 64);
            Square::new(idx as u8)
        };

        let dest = unsafe {
            let idx = (mv16 >> 4) & 63;
            debug_assert!(idx < 64);
            Square::new(idx as u8)
        };

        let promotion = if (mv16 & (1 << 2)) != 0 {
            Some(ALL_PIECES[((mv16 & 3) + 1) as usize])
        } else {
            None
        };

        ChessMove::new(src, dest, promotion)
    }
}
pub type Key16 = u16;
pub type GenBound8 = u8;
pub type Depth8 = u8;
pub type TTHandle = (usize, usize);

#[derive(Clone, Copy, Default)]
#[repr(C, align(8))]
pub struct TTEntry {
    pub key16: Key16,
    pub mv: Move16,
    pub eval: Eval,
    pub depth: Depth8,
    pub genbound: GenBound8,
}

impl From<u64> for TTEntry {
    fn from(uint: u64) -> Self {
        unsafe {
            mem::transmute::<u64, TTEntry>(uint)
        }
    }
}

impl From<TTEntry> for u64 {
    fn from(entry: TTEntry) -> Self {
        unsafe {
            mem::transmute::<TTEntry, u64>(entry)
        }
    }
}

impl TTEntry {
    pub fn new(key16: Key16, mv: Move16, eval: Eval, depth: Depth8, genbound: GenBound8) -> Self {
        TTEntry {
            key16,
            mv,
            eval,
            depth,
            genbound
        }
    }
    pub fn entry_type(&self) -> EntryType {
        EntryType::from(self.genbound & (!0 << 3))
    }
}
const CLUSTER_SIZE: usize = 4;

#[derive(Default)]
#[repr(C, align(32))]
struct TTCluster {
    pub entries: [AtomicU64; CLUSTER_SIZE],
}

impl TTCluster {
    fn get_entry(&self, idx: usize) -> TTEntry {
        self.entries[idx].load(Ordering::SeqCst).into()
    }

    fn save_entry<T>(&self, idx: usize, new_entry: T)
    where T: Into<u64> {
        self.entries[idx].store(new_entry.into(), Ordering::SeqCst);
    }
}

const GEN_BITS: u8 = 3;
const GEN_DELTA: u8 = 1 << GEN_BITS;
const GEN_CYCLE: u16 = 0xFF + (GEN_DELTA as u16);
const GEN_MASK: u16 = (0xFF << GEN_BITS) & 0xFF;

pub struct TTable {
    cluster_count: u64,
    gen8: AtomicU8,
    table: Vec<TTCluster>
}

impl TTable {
    pub fn new(mb_size: u64) -> Self {
        let cluster_count = (mb_size * (1 << 20)) / mem::size_of::<TTCluster>() as u64;
        let gen8 = AtomicU8::default();
        let mut table = Vec::new();
        table.extend(iter::repeat_with(TTCluster::default).take(cluster_count as usize));

        TTable {
            cluster_count,
            gen8,
            table,
        }
    }

    pub fn probe(&self, board: &Board) -> (Option<TTEntry>, TTHandle) {
        let hash = board.get_hash();
        let cluster_idx = self.get_cluster_idx(hash);
        let cluster = self.get_cluster(cluster_idx);

        let key16: Key16 = hash as Key16;
        let mut replace_idx = 0;
        let mut replace_entry: Option<TTEntry> = None;



        for idx in 0..CLUSTER_SIZE {
            let mut entry: TTEntry = cluster.get_entry(idx);
            if (entry.key16 == key16) || entry.depth != 0 {
                entry.genbound = (self.gen8() | (entry.genbound & (GEN_DELTA - 1)));
                cluster.save_entry(idx, entry);

                if entry.depth != 0 {
                    return (Some(entry), (cluster_idx, idx));
                } else {
                    return (None, (cluster_idx, idx));
                }
            } else if let Some(replace) = replace_entry.as_ref() {

                if self.entry_age(replace) > self.entry_age(&entry) {
                    replace_entry = Some(entry);
                    replace_idx = idx;
                }
            } else {
                replace_entry = Some(entry);;
            }

        }

        (None, (cluster_idx, replace_idx))
    }

    pub fn save<T>(&self, handle: TTHandle, board: &Board, mv: T, eval: Eval, depth: Depth8, et: EntryType)
        where T: Into<Move16>
    {
        let key = board.get_hash();
        let entry = self.get_entry(handle);
        if depth > entry.depth {
            let mv16: Move16 = mv.into();
            let key16 = key as Key16;
            let new = TTEntry::new(key16, mv16, eval, depth, self.gen8() | u8::from(et));
            self.save_entry(handle, new);
        }
    }

    pub fn new_search(&self) {
        self.gen8.fetch_add(GEN_DELTA, Ordering::Relaxed);
    }

    fn gen8(&self) -> u8 {
        self.gen8.load(Ordering::Relaxed)
    }

    fn get_cluster(&self, idx: usize) -> &TTCluster {
        unsafe {
            self.table.get_unchecked(idx)
        }
    }

    fn get_entry(&self, handle: TTHandle) -> TTEntry {
        let (cluster_idx, idx) = handle;

        let cluster = self.get_cluster(cluster_idx);
        cluster.entries[idx].load(Ordering::Acquire).into()
    }

    fn save_entry(&self, handle: TTHandle, entry: TTEntry) {
        let (cluster_idx, idx) = handle;

        let cluster = self.get_cluster(cluster_idx);

        cluster.entries[idx].store(entry.into(), Ordering::Release);
    }

    fn entry_age(&self, entry: &TTEntry) -> u8 {
        let age: u8 = ((GEN_CYCLE + self.gen8() as u16 - entry.genbound as u16) & GEN_MASK) as u8;

        entry.depth.wrapping_sub(age)
    }

    fn get_cluster_idx(&self, hash: u64) -> usize {
        let idx: usize = mul_hi_64(hash, self.cluster_count);
        debug_assert!(idx < self.table.len());
        idx
    }
}

fn mul_hi_64(x: u64, y: u64) -> usize {
    let xy: u128 = x as u128 * y as u128;
    (xy >> 64) as usize
}

//pub type CacheTable = Arc<RwLock<HashMap<Board, TableEntry>>>;


#[derive(Eq, Clone, Copy, Debug)]
pub struct EvalMove {
    pub mv: ChessMove,
    pub eval: Eval,
}

impl EvalMove {
    pub fn new(mv: ChessMove, eval: Eval ) -> Self {
        EvalMove {
            mv,
            eval,
        }
    }

    pub fn new_on_board(mv: ChessMove, board: &Board) -> Self {
        let pos = board.make_move_new(mv);
        Self::new(mv, -eval::evaluate_board(&pos))
    }
}


impl PartialEq for EvalMove {
    fn eq(&self, other: &Self) -> bool {
        self.eval.eq(&other.eval)
    }
}

impl PartialOrd for EvalMove {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EvalMove {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.eval.cmp(&other.eval)
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum EntryType {
    Pv = 1 << 2,
    Cut = 1 << 1,
    All = 1 << 0
}

impl From<u8> for EntryType {
    fn from(uint: u8) -> Self {
        unsafe {
            mem::transmute::<u8, EntryType>(uint)
        }
    }
}

impl From<EntryType> for u8 {
    fn from(et: EntryType) -> Self {
        unsafe {
            mem::transmute::<EntryType, u8>(et)
        }
    }
}



#[cfg(test)]
mod test {
    use super::*;
    use chess::PROMOTION_PIECES;

    #[test]
    fn test_sizes() {
        assert_eq!(mem::size_of::<TTCluster>(), 32);
        assert_eq!(mem::size_of::<AtomicU64>(), 8);
    }

    #[test]
    fn test_move_conversion() {
        for src in 0..64 {
            for dst in 0..64 {
                if dst == src {
                    continue;
                }
                let (src_sq, dst_sq) = unsafe {
                    (Square::new(src), Square::new(dst))
                };
                let non_prom = ChessMove::new(src_sq, dst_sq, None);
                assert!(conversion_works(non_prom));

                for piece in PROMOTION_PIECES {
                    let mv = ChessMove::new(src_sq, dst_sq, Some(piece));
                    assert!(conversion_works(mv));
                }
            }
        }

    }

    fn conversion_works(mv: ChessMove) -> bool {
        let mv16: Move16 = mv.into();

        let chess_move: ChessMove = mv16.into();

        println!("{:?} -> {:#018b} -> {:?}", mv, u16::from(mv16), chess_move);
        if mv == chess_move {
            true
        } else {
            false
        }
    }
}