use chess::{Board, ChessMove, MoveGen};
use super::eval;

pub fn choose_move(board: &Board) -> ChessMove {

    let mut max: i32 = -99999;
    let mut alpha = -100000;
    let mut beta = 100000;
    let mut best_move : Option<ChessMove> = None;

    let legal = MoveGen::new_legal(board);
    for mv in legal {
        let eval = -alphabeta(board.make_move_new(mv), -beta, -alpha,  5);
        if eval > max {
            max = eval;
            best_move = Some(mv);
        }

        if eval > alpha {
            alpha = eval;
        }
    }

    best_move.unwrap()
}

fn alphabeta(board: Board, alpha: i32, beta: i32, depth: u8) -> i32 {
    if depth == 0 {
        return eval::evaluate_board(board);
    }

    let mut alpha = alpha;
    let mut beta = beta;

    for mv in MoveGen::new_legal(&board) {
        let score = -alphabeta(board.make_move_new(mv), -beta, -alpha, depth - 1);

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
        return eval::evaluate_board(board);
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
