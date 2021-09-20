use chess::{Board, ChessMove, MoveGen, Color, BitBoard, BoardStatus};
use super::eval;

use log::{info, trace};
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::ops::{Range, Deref};
use std::iter;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::sync::atomic;
use std::sync::mpsc::Sender;
use std::cell::RefCell;

use vampirc_uci::UciMessage;
use std::cmp::{Ordering, Reverse};

pub type CacheTable = HashMap<Board, TableEntry>;

#[derive(Eq, Clone, Copy, Debug)]
pub struct EvalMove {
    pub mv: ChessMove,
    pub eval: i32,
}

impl EvalMove {
    fn new(mv: ChessMove, eval: i32 ) -> Self {
        EvalMove {
            mv,
            eval,
        }
    }

    fn new_on_board(mv: ChessMove, board: &Board) -> Self {
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
    fn new(best_move: EvalMove, old_depth: u8, entry_type: EntryType) -> Self {
        TableEntry {
            best_move,
            old_depth,
            entry_type,
        }
    }
}

static SCORE_MATE: i32 = 10_000_000;

fn acquire_read(lock: &Arc<RwLock<CacheTable>>) -> RwLockReadGuard<CacheTable> {
    //info!("Waiting for read lock...");
    let ret = lock.read().unwrap();
    //info!("Lock acquired.");
    ret
}

fn acquire_write(lock: &Arc<RwLock<CacheTable>>) -> RwLockWriteGuard<CacheTable> {
    //info!("Waiting for write lock...");
    let ret = lock.write().unwrap();
    //info!("Lock acquired.");
    ret
}
pub fn search(board: Board,
              cache: Arc<RwLock<CacheTable>>,
              moves: Option<Vec<ChessMove>>,
              depth: Option<u8>,
              stop: Arc<atomic::AtomicBool>,
              tx: Sender<UciMessage>)
{
    let mut move_order: Vec<EvalMove> = Vec::new();


    let (mut best_move, start) = {
        let lock = acquire_read(&cache);
        match lock.get(&board) {
            Some(&TableEntry { best_move, old_depth, entry_type }) => {
                (Some(best_move.to_owned()), old_depth.to_owned())
            }
            None => {
                (None, 0)
            }
        }
    };

    let end = if let Some(depth) = depth {
        depth - 1
    } else {
        255
    };

    let mut max = -99_999;

    for depth in start..=end {
        let mut alpha = -100_000;
        let mut beta = 100_000;

        let mut to_eval: Vec<EvalMove> = if (depth > start) {
            move_order.sort_by_key(|&em| Reverse(em));
            move_order.drain(..).collect()
        } else if let Some(moves) = moves.as_ref() {
            moves.into_iter().map(|&mv| EvalMove::new_on_board(mv, &board)).collect()
        } else {
            order_moves(&board, best_move.as_ref())
        };

        info!("searching: depth {}, {} legal moves", depth, to_eval.len());

        for EvalMove {mv, eval} in to_eval.drain(..) {
            if stop.load(atomic::Ordering::Acquire) {
                break;
            }

            let pos = board.make_move_new(mv);

            let eval = -alphabeta(pos, &cache,  -beta, -alpha, depth, 1);
            info!("eval {}: {}(depth {})", mv, eval, depth);

            if eval > max {
                info!("new best move: {}", mv);
                max = eval;
                if eval > alpha {
                    alpha = eval;
                }

                best_move = Some(EvalMove::new(mv, eval));
            }

            move_order.push(EvalMove::new(mv, eval));
        }


        if stop.load(atomic::Ordering::Acquire) {
            break;
        } else if let Err(_) = tx.send(UciMessage::best_move(best_move.unwrap().mv)) {
            break;
        }
    }

    stop.store(true, atomic::Ordering::Release);
}

fn order_moves(board: &Board, best_move: Option<&EvalMove>) -> Vec<EvalMove> {
    let mut legal = MoveGen::new_legal(board);

    let mut rest : Vec<EvalMove> = legal.filter_map(|mv| {
        let pos = board.make_move_new(mv);
        if let Some(em) = best_move {
            if em.mv == mv {
                return None;
            }
        }
        Some(EvalMove::new(mv, -eval::evaluate_board(&pos)))
    }).collect();

    let prelude = if let Some(&em) = best_move {
        vec![em]
    } else {
        Vec::new()
    };

    rest.sort_unstable_by_key(|em| Reverse(*em));

    prelude.into_iter().chain(rest.into_iter()).collect()
}

fn alphabeta(board: Board,
             cache: &Arc<RwLock<CacheTable>>,
             mut alpha: i32,
             mut beta: i32,
             depth: u8,
             root_distance: u8) -> i32
{
    if depth == 0 {
        return quiesce(board, alpha, beta);
        //return eval::evaluate_board(&board);
    }

    let table_entry = {
        let lock = acquire_read(cache);
        lock.get(&board).map(&ToOwned::to_owned)
    };

    if board.status() == BoardStatus::Checkmate {
        return -SCORE_MATE;
    }

    let winning_mate_score = SCORE_MATE - root_distance as i32;
    let losing_mate_score = -SCORE_MATE + root_distance as i32;

    let mut max = -99_999;
    let mut position_unseen = true;

    let mut best_move = if let Some(TableEntry { best_move, old_depth, entry_type}) = table_entry {
        if old_depth >= depth {
            /* we already have a deeper evaluation cached, so just return it. */
            return best_move.eval;
        } else {
            position_unseen = false;
            Some(best_move.to_owned())
        }
    } else {
        None
    };

    let legal = order_moves(&board, best_move.as_ref());



    for em in legal.iter() {
        let &EvalMove {mv, eval} = em;
        let pos = board.make_move_new(mv);

        /* If it's the principal variation, do a full search.
         * Otherwise, do a null window search to see if
         * an improvement is possible.
         * If the position is previously unseen, do a regular alpha/beta search.
         */
        let score = if position_unseen {
            -alphabeta(pos, cache, -beta, -alpha, depth - 1, root_distance + 1)
        } else if legal.starts_with(&[*em]) {
            -alphabeta(pos, cache, -beta, -alpha, depth - 1, root_distance + 1)
        } else if -alphabeta(pos, cache, -alpha - 1, -alpha, depth - 1, root_distance + 1) > alpha {
            -alphabeta(pos, cache, -beta, -alpha, depth - 1, root_distance + 1)
        } else {
            alpha
        };

        //info!("{}eval {}: {}(depth {})", indentation, mv, score, depth);

        if score >= beta {
            return score;
            //return quiesce(board, alpha, beta);
        }

        if score > max {
            best_move = Some(EvalMove::new(mv, score));
            max = score;
            if score > alpha {
                alpha = score;
            }
        }

        //mate pruning
        if winning_mate_score < beta {
            beta = winning_mate_score;
            if alpha >= winning_mate_score {
                return winning_mate_score;
            }
        }

        if losing_mate_score > alpha {
            alpha = losing_mate_score;
            if beta <= losing_mate_score {
                return losing_mate_score;
            }
        }
    }


    if let Some(em) = best_move {
        acquire_write(&cache).insert(board, TableEntry::new(em, depth, EntryType::Pv));
    }

    if max > 1_000_000 {
        max - 1
    } else if max < -1_000_000 {
        max + 1
    } else {
        max
    }
}

static DELTA_MARGIN: i32 = 200;

fn quiesce(board: Board, mut alpha: i32, beta: i32) -> i32 {
    let cur_eval = eval::evaluate_board(&board);

    if cur_eval >= beta {
        return beta;
    }

    if alpha < cur_eval {
        alpha = cur_eval;
    }

    let min_color = match board.side_to_move() {
        Color::White => Color::Black,
        Color::Black => Color::White,
    };

    let mut captures = MoveGen::new_legal(&board);
    captures.set_iterator_mask(*board.color_combined(min_color));

    for mv in captures {
        let score = -quiesce(board.make_move_new(mv), -beta, -alpha);

        if score >= beta {
            return beta;
        } else if score > alpha {
            alpha = score;
        }

    }

    alpha
}