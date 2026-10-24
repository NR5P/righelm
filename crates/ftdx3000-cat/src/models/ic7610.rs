//! Icom IC-7610 (placeholder: being written; behaves like an IC-7300).

use crate::model::RadioModel;

pub static IC7610: RadioModel = RadioModel { key: "ic7610", name: "IC-7610", civ_addr: 0x98, hamlib: 3078, ..super::ic7300::IC7300 };
