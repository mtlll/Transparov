use chess::{Board, ChessMove, MoveGen, Color};
use super::eval;

use log::info;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::ops::Range;

#[derive(Eq, Clone, Copy)]
struct EvalMove {
    mv: ChessMove,
    eval: i32,
}
impl EvalMove {
    fn new(mv: ChessMove, eval: i32) -> Self {
        EvalMove {
            mv,
            eval
        }
    }
}


impl PartialEq for EvalMove {
    fn eq(&self, other: &Self) -> bool {
       self.eval == self.eval
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

pub fn search(board: &Board, moves: Option<Vec<ChessMove>>, depth: u8) -> (ChessMove, i32) {

    let mut max = -99999;
    let mut bestmove = ChessMove::default();
    let mut alpha = -100000;
    let mut beta = 100000;

    let mut to_eval : Vec<ChessMove> = moves.unwrap_or_else(|| {
        MoveGen::new_legal(board).collect()
    });
    info!("searching: depth {}, {} legal moves", depth, to_eval.len());

    for mv in to_eval.drain(..) {
        let pos = board.make_move_new(mv);
        let eval = -alphabeta(pos, -beta, -alpha, depth - 1);
        info!("eval {}: {}(depth {})", mv, eval, depth);

        if eval > max {
            info!("new best move: {}", mv);
            max = eval;

            if eval > alpha {
                alpha = eval;
            }
        }

    }

    (bestmove, max)
}

fn alphabeta(board: Board, alpha: i32, beta: i32, depth: u8) -> i32 {
    if depth == 0 {
        //return quiesce(board, alpha, beta);
        return eval::evaluate_board(&board);
    }

    let mut alpha = alpha;
    let mut beta = beta;

    let legal = MoveGen::new_legal(&board);
    if board.checkers().popcnt() > 0 && legal.len() == 0 {
        return beta;
    }

    let mut bestscore: i32 = -99999;

    for mv in MoveGen::new_legal(&board) {
        let score = -alphabeta(board.make_move_new(mv), -beta, -alpha, depth - 1);

        if score >= beta {
            return score;
            //return quiesce(board, alpha, beta);
        }

        if score > bestscore {
            bestscore = score;
            if score > alpha {
                alpha = score;
            }
        }
    }

    bestscore
}

fn quiesce(board: Board, alpha: i32, beta: i32) -> i32 {
    let mut alpha = alpha;
    let cur_eval = eval::evaluate_board(&board);

    if cur_eval >= beta {
        return beta;
    } else {
        alpha = cur_eval;
    }

    let min_color = match board.side_to_move() {
        Color::White => Color::Black,
        Color::Black => Color::White,
    };

    let mut legal = MoveGen::new_legal(&board);
    legal.set_iterator_mask(*board.color_combined(min_color));

    for mv in legal {
        let score = -quiesce(board.make_move_new(mv), -beta, -alpha);

        if score >= beta {
            return beta;
        } else if score > alpha {
            alpha = score;
        }
    }

    alpha
}
fn negamax(board: Board, depth: u8) -> i32 {
    if depth == 0 {
        return eval::evaluate_board(&board);
    }

    let mut max: i32 = -9999;

    for mv in MoveGen::new_legal(&board) {
        let score = -negamax(board.make_move_new(mv), depth - 1);
        if score > max {
            max = score;
        }
    }

    max
}

#[cfg(test)]
mod tests {
    use super::EvalMove;
    use super::ChessMove;

    #[test]
    fn move_ordering() {
        assert!(-100 < 100);
        assert!(EvalMove::new(ChessMove::default(), -100) <
            EvalMove::new(ChessMove::default(), 100));
    }
}