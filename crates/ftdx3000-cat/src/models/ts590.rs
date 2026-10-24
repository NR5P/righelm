//! Placeholder (being written): behaves like an FTDX3000 with another name.

use crate::dialect::text::{Ctx, TextSpec, read_prefix};
use crate::model::{FTDX3000, Family, Maker, Proto, RadioModel};

pub static SPEC: TextSpec = TextSpec { down, up, answer_prefix: read_prefix };

fn down(cx: &mut Ctx, f: &str) -> Vec<String> {
    cx.same(f)
}

fn up(cx: &mut Ctx, f: &str) -> Vec<String> {
    cx.same(f)
}

pub static TS590S: RadioModel = RadioModel {
    key: "ts590s",
    name: "TS-590S",
    maker: Maker::Kenwood,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "021",
    hamlib: 2031,
    roles: crate::model::Roles::NONE,
    menu: &[],
    ..FTDX3000
};

pub static TS590SG: RadioModel = RadioModel {
    key: "ts590sg",
    name: "TS-590SG",
    maker: Maker::Kenwood,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "023",
    hamlib: 2037,
    roles: crate::model::Roles::NONE,
    menu: &[],
    ..FTDX3000
};
