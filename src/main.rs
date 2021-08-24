mod engine;
use engine::search;

use chess::Board;
use chess::MoveGen;
use chess::Color;
use chess::Game;
use chess::ChessMove;
use chess::Piece;

use std::process::exit;
use std::io;
use std::io::{stdout, stdin, Write};

fn main() {
    let mut game = Game::new();
    let colour = get_color();

    loop {
        if let Some(result) = game.result() {
            println!("{:?}", result);
            exit(0);
        } else if game.side_to_move() == colour {
           // let my_move = random_legal_move(&game.current_position(), &mut rng);
            let my_move = search::choose_move(&game.current_position());
            game.make_move(my_move);
            println!("{}", my_move);
        } else {
            let their_move = get_move(&game);
            game.make_move(their_move);
        }
    }
}


fn prompt(prompt: &str) -> String {
    let mut s = String::new();
    print!("{}", prompt);
    let _ = stdout().flush();
    stdin().read_line(&mut s).unwrap();
    //println!("{}", s);
    s.trim().to_string()
}

fn get_color() -> Color {
    loop {
        let reply = prompt("Choose a color(white/black/random): ").to_lowercase();
        match reply.as_str() {
            "white" => {
                return Color::Black;
            },
            "black" => {
                return Color::White;
            },
            _ => {
                println!("Invalid choice");
            },
        }
    }
}

fn get_move(game: &Game) -> ChessMove {
    loop {
        let reply = prompt("Make a move: ");
        match ChessMove::from_san(&game.current_position(), &reply) {
            Ok(mv) => {
                return mv;
            },
            Err(_) => {
                println!("Invalid move: {}", reply);
            }
        }
    }

}




fn move_to_san(mv: &ChessMove, board: &Board) -> String {
    let src = mv.get_source();
    let dst = mv.get_dest();

    String::new()
}