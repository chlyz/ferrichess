//! Test-only legal move sequences from the CC0 Lichess Open Database.
//!
//! Source file, checksum, game identifiers, and derivation policy are recorded
//! in `../../../docs/test-data-provenance.md`.

#![allow(dead_code)]

//! CC0 Lichess Open Database move prefixes shared by tests.
//!
//! Their game URLs, pinned export checksum, and copied prefixes are recorded
//! in `docs/test-data-provenance.md`.

pub const FRENCH_PREFIX: &str = "1. e4 e6 2. d4 b6 3. a3 Bb7";
pub const FRENCH_MATE: &str = "1. e4 e6 2. d4 b6 3. a3 Bb7 4. Nc3 Nh6 5. Bxh6 gxh6 6. Be2 Qg5 7. Bg4 h5 8. Nf3 Qg6 9. Nh4 Qg5 10. Bxh5 Qxh4 11. Qf3 Kd8 12. Qxf7 Nc6 13. Qe8#";
pub const COLLE_PREFIX: &str = "1. d4 d5 2. Nf3 Nf6 3. e3 Bf5";
pub const ITALIAN_PREFIX: &str = "1. e4 e5 2. Nf3 Nc6 3. Bc4 Nf6 4. Nc3 Bc5";
pub const ITALIAN_CHECK: &str = "1. e4 e5 2. Nf3 Nc6 3. Bc4 Nf6 4. Nc3 Bc5 5. a3 Bxf2+ 6. Kxf2 Nd4 7. d3 Ng4+ 8. Kf1 Qf6 9. h3 d5 10. Nxd5 Qe6 11. Nxc7+";
pub const CARO_KANN_PREFIX: &str = "1. e4 c6 2. Nc3 d5 3. Qf3 dxe4 4. Nxe4 Nd7";
pub const ENGLUND_PREFIX: &str = "1. d4 e5 2. dxe5 d6 3. exd6 Bxd6 4. Nf3 Nf6";
