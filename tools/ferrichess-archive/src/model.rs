use ferrichess_games::Game;

#[derive(Debug)]
pub struct ImportRecord {
    pub source: &'static str,
    pub game_id: String,
    pub site: String,
    pub played_at: String,
    pub time_class: String,
    pub rated: bool,
    pub white_rating: Option<i64>,
    pub black_rating: Option<i64>,
    pub pgn: String,
    pub game: Game,
}
