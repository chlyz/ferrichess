use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

use crate::AppResult;

#[derive(Debug)]
pub struct Evaluation {
    pub name: String,
    pub depth: u32,
    pub nodes: u64,
    pub pvs: Vec<Pv>,
}

#[derive(Debug)]
pub struct Pv {
    pub moves: String,
    pub cp: Option<i32>,
    pub mate: Option<i32>,
}

pub fn analyse(
    fen: &str,
    moves: &[String],
    depth: u8,
    requested_lines: u8,
) -> AppResult<Option<Evaluation>> {
    let mut child = match Command::new("stockfish")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut stdin = child.stdin.take().ok_or("Stockfish stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("Stockfish stdout unavailable")?;
    let multipv = if moves.is_empty() {
        usize::from(requested_lines).clamp(1, 10)
    } else {
        moves.len().clamp(1, 10)
    };
    writeln!(stdin, "uci")?;
    writeln!(stdin, "setoption name MultiPV value {multipv}")?;
    writeln!(stdin, "isready")?;
    writeln!(stdin, "position fen {fen}")?;
    if moves.is_empty() {
        writeln!(stdin, "go depth {depth}")?;
    } else {
        writeln!(stdin, "go depth {depth} searchmoves {}", moves.join(" "))?;
    }
    stdin.flush()?;

    let black_to_move = fen.split_whitespace().nth(1) == Some("b");
    let mut name = "Stockfish".to_owned();
    let mut lines = BTreeMap::new();
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        if let Some(value) = line.strip_prefix("id name ") {
            name = value.to_owned();
        } else if line.starts_with("info ") {
            if let Some((multipv, depth, nodes, mut pv)) = parse_info(&line) {
                if black_to_move {
                    pv.cp = pv.cp.map(|score| -score);
                    pv.mate = pv.mate.map(|score| -score);
                }
                lines.insert(multipv, (depth, nodes, pv));
            }
        } else if line.starts_with("bestmove ") {
            break;
        }
    }
    writeln!(stdin, "quit")?;
    let _ = child.wait();
    let final_depth = lines.values().map(|item| item.0).max().unwrap_or(0);
    let nodes = lines.values().map(|item| item.1).max().unwrap_or(0);
    Ok(Some(Evaluation {
        name,
        depth: final_depth,
        nodes,
        pvs: lines.into_values().map(|item| item.2).collect(),
    }))
}

fn parse_info(line: &str) -> Option<(u32, u32, u64, Pv)> {
    let words: Vec<_> = line.split_whitespace().collect();
    let value = |key: &str| {
        words
            .iter()
            .position(|word| *word == key)
            .and_then(|index| words.get(index + 1).copied())
    };
    let depth = value("depth")?.parse().ok()?;
    let multipv = value("multipv").unwrap_or("1").parse().ok()?;
    let nodes = value("nodes").unwrap_or("0").parse().ok()?;
    let score_index = words.iter().position(|word| *word == "score")?;
    let score_type = *words.get(score_index + 1)?;
    let score: i32 = words.get(score_index + 2)?.parse().ok()?;
    let pv_index = words.iter().position(|word| *word == "pv")?;
    let moves = words[pv_index + 1..].join(" ");
    Some((
        multipv,
        depth,
        nodes,
        Pv {
            moves,
            cp: (score_type == "cp").then_some(score),
            mate: (score_type == "mate").then_some(score),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_stockfish_multipv_line() {
        let (_, depth, nodes, pv) = parse_info(
            "info depth 18 seldepth 27 multipv 2 score cp -22 nodes 12345 nps 1 pv f6g4 g5h3",
        )
        .expect("info line");
        assert_eq!(depth, 18);
        assert_eq!(nodes, 12345);
        assert_eq!(pv.cp, Some(-22));
        assert_eq!(pv.moves, "f6g4 g5h3");
    }
}
