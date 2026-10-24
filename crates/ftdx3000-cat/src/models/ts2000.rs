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

pub static TS2000: RadioModel = RadioModel {
    key: "ts2000",
    name: "TS-2000",
    maker: Maker::Kenwood,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "019",
    hamlib: 2014,
    roles: crate::model::Roles::NONE,
    menu: &[],
    ..FTDX3000
};
