use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use chess::{Board, ChessMove};

use crate::engine::eval;

#[derive(Clone)]
pub struct CacheTable(Arc<RwLock<HashMap<Board, TableEntry>>>);

//pub type CacheTable = Arc<RwLock<HashMap<Board, TableEntry>>>;

impl Default for CacheTable {
    fn default() -> Self {
        CacheTable(Arc::new(RwLock::new(HashMap::new())))
    }
}

impl CacheTable {
    pub fn acquire_read(&self) -> RwLockReadGuard<HashMap<Board, TableEntry>> {
        //info!("Waiting for read lock...");
        let ret = self.0.read().unwrap();
        //info!("Lock acquired.");
        ret
    }

    pub fn acquire_write(&self) -> RwLockWriteGuard<HashMap<Board, TableEntry>> {
        //info!("Waiting for write lock...");
        let ret = self.0.write().unwrap();
        //info!("Lock acquired.");
        ret
    }

    pub fn save(&self,
                board: &Board,
                best_move: EvalMove,
                depth: u8,
                entry_type: EntryType) {
        let new_entry = TableEntry::new(best_move, depth, entry_type);
        let mut lock = self.acquire_write();
        lock.insert(board.clone(), new_entry);
    }

    pub fn probe(&self, board: &Board) -> Option<TableEntry> {
        let lock = self.acquire_read();
        lock.get(board).map(ToOwned::to_owned)
    }
}
#[derive(Eq, Clone, Copy, Debug)]
pub struct EvalMove {
    pub mv: ChessMove,
    pub eval: i32,
}

impl EvalMove {
    pub fn new(mv: ChessMove, eval: i32 ) -> Self {
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
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EvalMove {
    fn cmp(&self, other: &Self) -> Ordering {
        self.eval.cmp(&other.eval)
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub enum EntryType {
    Pv,
    Cut,
    All,
}

#[derive(Clone, Copy)]
pub struct TableEntry {
    pub best_move: EvalMove,
    pub old_depth: u8,
    pub entry_type: EntryType,
}

impl TableEntry {
    pub fn new(best_move: EvalMove, old_depth: u8, entry_type: EntryType) -> Self {
        TableEntry {
            best_move,
            old_depth,
            entry_type,
        }
    }
}


