//! The table of supported radios. Each file has one radio (or a few close
//! relatives): what it has, its menu, and how its commands differ from the
//! FTDX3000's. See [`crate::model`] for what the fields mean and
//! [`crate::dialect::text`] for the translating.

use crate::model::{FTDX3000, RadioModel};

pub mod ftdx101;
pub mod ftdx10;
pub mod ft710;
pub mod ft991a;
pub mod ft891;
pub mod ftdx5000;
pub mod ftdx1200;
pub mod ft950;
pub mod ft2000;
pub mod ft450d;
pub mod ts590;
pub mod ts890;
pub mod ts990;
pub mod ts2000;
pub mod ts480;
pub mod k3;
pub mod k4;
pub mod ic7300;
pub mod ic7610;
pub mod ic705;
pub mod ic9700;

/// Every supported radio, FTDX3000 first. Where several share an `ID;`
/// answer, the first listed is the one detected.
pub static ALL: &[&RadioModel] = &[
    &FTDX3000,
    &ftdx101::FTDX101D,
    &ftdx101::FTDX101MP,
    &ftdx10::FTDX10,
    &ft710::FT710,
    &ft991a::FT991A,
    &ft991a::FT991,
    &ft891::FT891,
    &ftdx5000::FTDX5000,
    &ftdx1200::FTDX1200,
    &ft950::FT950,
    &ft2000::FT2000,
    &ft450d::FT450D,
    &ft450d::FT450,
    &ts590::TS590S,
    &ts590::TS590SG,
    &ts890::TS890S,
    &ts990::TS990S,
    &ts2000::TS2000,
    &ts480::TS480,
    &k3::K3,
    &k3::KX3,
    &k3::KX2,
    &k4::K4,
    &ic7300::IC7300,
    &ic7300::IC7300MK2,
    &ic7610::IC7610,
    &ic705::IC705,
    &ic9700::IC9700,
];
