use chess::{Board, ChessMove, MoveGen, Color, BitBoard};
use super::eval;

use log::info;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::ops::{Range, Deref};
use std::iter;

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

static SCORE_MATE: i32 = 1000000;
pub fn search(board: &Board, moves: Option<Vec<ChessMove>>, depth: u8) -> (ChessMove, i32) {

    let mut max = -9999999;
    let mut bestmove = ChessMove::default();
    let mut alpha = -1000000;
    let mut beta = 1000000;

    let mut to_eval : Vec<ChessMove> = moves.unwrap_or_else(|| {
        order_moves(board)
    });
    info!("searching: depth {}, {} legal moves", depth, to_eval.len());

    for mv in to_eval.drain(..) {
        let pos = board.make_move_new(mv);
        let eval = -alphabeta(pos, -beta, -alpha, depth - 1, 1);
        info!("eval {}: {}(depth {})", mv, eval, depth);

        if eval > max {
            info!("new best move: {}", mv);
            max = eval;
            bestmove = mv;

            if eval > alpha {
                alpha = eval;
            }
        }

    }

    (bestmove, max)
}

fn order_moves(board: &Board) -> Vec<ChessMove> {
    let mut captures = MoveGen::new_legal(board);
    let mut other = MoveGen::new_legal(board);

    let occupied = *board.color_combined(!board.side_to_move());
    let empty = !*board.combined();

    captures.set_iterator_mask(occupied);
    other.set_iterator_mask(empty);

    captures.chain(other).collect()

}
fn alphabeta(board: Board, alpha: i32, beta: i32, depth: u8, indent: u8) -> i32 {
    if depth == 0 {
        return quiesce(board, alpha, beta);
        //return eval::evaluate_board(&board);
    }

    let indentation : String = iter::repeat("\t").take(indent as usize ).collect();

    let mut alpha = alpha;
    let mut beta = beta;

    let legal = order_moves(&board);
    if board.checkers().popcnt() > 0 && legal.len() == 0 {
        return -SCORE_MATE;
    }

    let mut bestscore: i32 = -99999;

    for mv in legal {
        let score = -alphabeta(board.make_move_new(mv), -beta, -alpha, depth - 1, indent + 1);

        //info!("{}eval {}: {}(depth {})", indentation, mv, score, depth);

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

    if bestscore > 100000 {
        bestscore - 1
    } else if bestscore < - 100000 {
        bestscore + 1
    } else {
        bestscore
    }
}
static DELTA_MARGIN: i32 = 200;
fn quiesce(board: Board, alpha: i32, beta: i32) -> i32 {
    let mut alpha = alpha;
    let cur_eval = eval::evaluate_board(&board);

    if cur_eval >= beta {
        return beta;
    } else if alpha < cur_eval {
        alpha = cur_eval;
    }

    let min_color = match board.side_to_move() {
        Color::White => Color::Black,
        Color::Black => Color::White,
    };

    let mut legal = MoveGen::new_legal(&board);
    legal.set_iterator_mask(*board.color_combined(min_color));

    for mv in legal {
        let piece = board.piece_on(mv.get_dest()).unwrap();
        if cur_eval + eval::PIECE_VALUES[piece.to_index()] + DELTA_MARGIN < alpha {
            continue;
        }
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