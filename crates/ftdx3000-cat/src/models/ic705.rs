//! Icom IC-705 (placeholder: being written; behaves like an IC-7300).

use crate::model::RadioModel;

pub static IC705: RadioModel = RadioModel { key: "ic705", name: "IC-705", civ_addr: 0xA4, hamlib: 3085, ..super::ic7300::IC7300 };
