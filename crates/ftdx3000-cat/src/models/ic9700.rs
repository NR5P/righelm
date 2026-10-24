//! Icom IC-9700 (placeholder: being written; behaves like an IC-7300).

use crate::model::RadioModel;

pub static IC9700: RadioModel = RadioModel { key: "ic9700", name: "IC-9700", civ_addr: 0xA2, hamlib: 3081, ..super::ic7300::IC7300 };
