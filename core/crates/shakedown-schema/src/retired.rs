//! Migrations an unreleased build of shaked applied (branch `factory`, 2026-11-20) and that no
//! release ships: the identity stream and shakedown 2–3 with organisations. A database that has
//! any of them is converted by hand (the hosted M1 plan's step O1) before this shaked opens it.

use crate::{stream::hex, Applied};

/// (stream, version, sha256 hex) of each retired file.
pub const RETIRED: &[(&str, u32, &str)] = &[
    ("shakedown", 2, "34552019c2a5a894b68b5c1007c139dbf5bfd559a88f37cca0811394dc31c68b"),
    ("shakedown", 3, "24fce61a85a57593c4cdb503551bc810349315eeeac66ce9c68c240ab050d3b5"),
    ("identity", 1, "2c29d43e2e4da0ed602400d0e5b34947373e2784bf3d19274bf41f712a1b778f"),
    ("identity", 2, "16e522f6d2dcfa5034004d87457d499a3d74cbce6c8b0335847430599f22cc99"),
];

/// True when `applied` holds a retired file, by stream, version and checksum.
pub fn retired_in(applied: &[Applied]) -> bool {
    applied.iter().any(|a| {
        RETIRED.iter().any(|(s, v, sum)| {
            a.stream.as_str() == *s && a.version == *v && hex(&a.checksum) == *sum
        })
    })
}
