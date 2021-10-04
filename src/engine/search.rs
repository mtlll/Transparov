use std::cmp::{Reverse, max, min};
use std::sync::Arc;
use std::sync::atomic;
use std::sync::mpsc::Sender;
use std::thread;
use std::panic::Location;
use chess::{Board, BoardStatus, ChessMove, Color, MoveGen};
use log::info;
use vampirc_uci::UciMessage;
use rayon::prelude::*;

use crate::engine::ttable::{CacheTable, EntryType, EvalMove, TTEntry};

use super::eval;
use eval::Eval;

const SCORE_MATE: Eval = 32_000;
const SCORE_INF: Eval = 32_001;

#[track_caller]
fn make_move_new(board: &Board, mv: ChessMove) -> Option<Board> {
    if !board.legal(mv) {
        let caller_loc = Location::caller();
        info!("Illegal move attempted in {} on line {}.", caller_loc.file(), caller_loc.line());
        None
    } else {
        Some(board.make_move_new(mv))
    }
}
pub fn search(board: Board,
              cache: CacheTable,
              moves: Option<Vec<ChessMove>>,
              depth: Option<u8>,
              stop: Arc<atomic::AtomicBool>,
              tx: Sender<UciMessage>)
{
    info!("searching from thread: {:?}", thread::current().id());
    let mut move_order: Vec<EvalMove> = Vec::new();


    let mut best_move = cache.probe(&board).0.map(|TTEntry {mv, eval, ..}| {
        EvalMove::new(mv.into(), eval)
    });

    let end = depth.unwrap_or(255);

    let mut max = Eval::MIN;

    for depth in 0..=end {

        let mut to_eval: Vec<EvalMove> = if depth > 0 {
            move_order.drain(..).collect()
        } else if let Some(moves) = moves.as_ref() {
            moves.into_iter().map(|&mv| EvalMove::new_on_board(mv, &board)).collect()
        } else {
            order_moves(&board, best_move.as_ref())
        };

        info!("searching: depth {}, {} legal moves", depth, to_eval.len());

        move_order = to_eval.par_iter().map(|&EvalMove {mv, eval}| {
            let pos = make_move_new(&board, mv).unwrap();
            let new_eval = -aspiration_search(pos, &cache.clone(), -eval, depth);
            info!("eval {}: new {}(depth {}) old {}", mv, new_eval, depth, eval);
            EvalMove::new(mv, new_eval)
        }).collect();

        move_order.sort_by_key(|&em| Reverse(em));


        best_move = move_order.get(0).cloned();
        /*
        for EvalMove {mv, eval} in to_eval.drain(..) {
            if stop.load(atomic::Ordering::Acquire) {
                break;
            }

            let pos = board.make_move_new(mv);

            let new_eval = -aspiration_search(&pos, &cache, -eval, depth);

            if new_eval > max {
                max = new_eval;
                if new_eval > alpha {
                    alpha = new_eval;
                }

                best_move = Some(EvalMove::new(mv, new_eval));
            }

            move_order.push(EvalMove::new(mv, new_eval));
        }

         */


        if stop.load(atomic::Ordering::Acquire) {
            break;
        } else if let Err(_) = tx.send(UciMessage::best_move(best_move.unwrap().mv)) {
            break;
        }
    }

    stop.store(true, atomic::Ordering::Release);
}

fn aspiration_search(board: Board, cache: &CacheTable, expected_eval: Eval, depth: u8) -> Eval {
    let mut delta: Eval = 17;

    let mut alpha: Eval = max(expected_eval - delta, -SCORE_INF);
    let mut beta: Eval = min(expected_eval + delta, SCORE_INF);

    let mut failed_high_count: u8 = 0;

    loop {
        let adjusted_depth: u8 = max(1, depth.saturating_sub(failed_high_count));
        let eval = alphabeta(board, cache, alpha, beta, depth, 1);

        if eval <= alpha {
            beta = min((alpha / 2).saturating_add(beta/2), SCORE_INF);
            alpha = max(eval.saturating_sub(delta), -SCORE_INF);
            failed_high_count = 0;
        } else if eval >= beta {
            beta = min(eval.saturating_add(delta), SCORE_INF);
            failed_high_count += 1;
        } else {
            return eval;
        }

        delta += delta / 4 + 5;
    }
}

fn order_moves(board: &Board, best_move: Option<&EvalMove>) -> Vec<EvalMove> {
    let legal = MoveGen::new_legal(board);

    let mut rest : Vec<EvalMove> = legal.filter_map(|mv| {
        if let Some(em) = best_move {
            if em.mv == mv {
                return None;
            }
        }
        let pos = board.make_move_new(mv);
        Some(EvalMove::new(mv, -eval::evaluate_board(&pos)))
    }).collect();

    let mut prelude = Vec::new();

    best_move.filter(|em| board.legal(em.mv)).map(|&em| {
        prelude.push(em);
    });

    rest.sort_unstable_by_key(|em| Reverse(*em));

    prelude.into_iter().chain(rest.into_iter()).collect()
}

fn alphabeta(board: Board,
             cache: &CacheTable,
             mut alpha: Eval,
             mut beta: Eval,
             depth: u8,
             root_distance: u8) -> Eval
{
    match board.status() {
        BoardStatus::Checkmate => {
            return -SCORE_MATE;
        },
        BoardStatus::Stalemate => {
            return 0;
        },
        _ => {},
    }

    if depth == 0 {
        return quiesce(board, alpha, beta);
        //return eval::evaluate_board(&board);
    }

    let mating_score = SCORE_MATE - root_distance as Eval;

    let mut max = Eval::MIN;

    let (table_entry, handle) = cache.probe(&board);
    let mut best_move = None;
    let mut tt_move: Option<ChessMove> = None;

    if let Some(te) = table_entry {

        if te.depth >= depth {
            /* we already have a deeper evaluation cached, so just return it. */
            return te.eval;
        } else {
            best_move = Some(EvalMove::new(te.mv.into(), te.eval));
            tt_move = Some(te.mv.into());
        }
    }

    let legal = order_moves(&board, best_move.as_ref());

    for em in legal.iter() {
        let &EvalMove {mv, eval} = em;
        let pos = if let Some(new_pos) = make_move_new(&board, mv).take() {
            new_pos
        } else {
            if  tt_move == Some(mv) {
                info!("Attempted move came from the TT");
            } else {
                info!("Attempted move did not come from the TT");
            }
            continue;
        };

        /* If it's the principal variation, do a full search.
         * Otherwise, do a null window search to see if
         * an improvement is possible.
         * If the position is previously unseen, do a regular alpha/beta search.
         */
        let score = -alphabeta(pos, cache, -beta, -alpha, depth - 1, root_distance + 1);


        //info!("{}eval {}: {}(depth {})", indentation, mv, score, depth);

        if score >= beta {
            cache.save(handle, &board, mv, score, depth, EntryType::Cut);
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
        if mating_score < beta {
            beta = mating_score;
            if alpha >= mating_score {
                return mating_score;
            }
        }

        if -mating_score > alpha {
            alpha = -mating_score;
            if beta <= -mating_score {
                return -mating_score;
            }
        }
    }

    if let Some(EvalMove {mv, eval}) = best_move {
        let entry_type = if max < alpha {
            EntryType::All
        } else {
            EntryType::Pv
        };

        cache.save(handle, &board, mv, eval, depth, entry_type);
    }

    if max >= SCORE_MATE - depth as Eval {
        max - 1
    } else if max < -SCORE_MATE + depth as Eval {
        max + 1
    } else {
        max
    }
}

static DELTA_MARGIN: Eval = 200;

fn quiesce(board: Board, mut alpha: Eval, beta: Eval) -> Eval {
    if board.status() == BoardStatus::Checkmate {
        return -SCORE_MATE;
    }
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