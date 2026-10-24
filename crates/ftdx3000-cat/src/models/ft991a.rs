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

pub static FT991A: RadioModel = RadioModel {
    key: "ft991a",
    name: "FT-991A",
    maker: Maker::Yaesu,
    family: Family::Yaesu,
    proto: Proto::Text(&SPEC),
    id: "0570",
    hamlib: 1035,
    roles: crate::model::Roles::NONE,
    menu: &[],
    ..FTDX3000
};
