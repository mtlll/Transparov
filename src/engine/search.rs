use chess::{Board, BoardStatus, ChessMove, Color, MoveGen};
use log::info;
use std::cmp::Reverse;
use std::panic::Location;

use crate::engine::ttable::{EntryType, EvalMove, TT};

use super::eval;
use eval::Eval;

pub(crate) const SCORE_MATE: Eval = 32_000;
pub(crate) const SCORE_INF: Eval = 32_001;

#[track_caller]
pub(crate) fn make_move_new(board: &Board, mv: ChessMove) -> Option<Board> {
    if !board.legal(mv) {
        let caller_loc = Location::caller();
        info!(
            "Illegal move attempted in {} on line {}.",
            caller_loc.file(),
            caller_loc.line()
        );
        None
    } else {
        Some(board.make_move_new(mv))
    }
}

fn order_moves(board: &Board, best_move: Option<&EvalMove>) -> Vec<EvalMove> {
    let legal = MoveGen::new_legal(board);

    let mut rest: Vec<EvalMove> = legal
        .filter_map(|mv| {
            if let Some(em) = best_move {
                if em.mv == mv {
                    return None;
                }
            }
            let pos = board.make_move_new(mv);
            Some(EvalMove::new(mv, -eval::evaluate_board(&pos)))
        })
        .collect();

    let mut prelude = Vec::new();

    if let Some(&em) = best_move.filter(|em| board.legal(em.mv)) {
        prelude.push(em);
    }

    rest.sort_unstable_by_key(|em| Reverse(*em));

    prelude.into_iter().chain(rest.into_iter()).collect()
}

pub fn alphabeta(
    board: Board,
    mut alpha: Eval,
    mut beta: Eval,
    depth: u8,
    root_distance: u8,
) -> Eval {
    match board.status() {
        BoardStatus::Checkmate => {
            return -SCORE_MATE;
        }
        BoardStatus::Stalemate => {
            return 0;
        }
        _ => {}
    }

    if depth == 0 {
        return quiesce(board, alpha, beta);
        //return eval::evaluate_board(&board);
    }

    let mating_score = SCORE_MATE - root_distance as Eval;

    let mut max = Eval::MIN;

    let (table_entry, handle) = TT.probe(&board);
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
        let &EvalMove { mv, eval } = em;
        let pos = if let Some(new_pos) = make_move_new(&board, mv).take() {
            new_pos
        } else {
            if tt_move == Some(mv) {
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
        let score = -alphabeta(pos, -beta, -alpha, depth - 1, root_distance + 1);

        //info!("{}eval {}: {}(depth {})", indentation, mv, score, depth);

        if score >= beta {
            TT.save(handle, &board, mv, score, depth, EntryType::Cut);
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

    if let Some(EvalMove { mv, eval }) = best_move {
        let entry_type = if max < alpha {
            EntryType::All
        } else {
            EntryType::Pv
        };

        TT.save(handle, &board, mv, eval, depth, entry_type);
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
