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

pub static FTDX10: RadioModel = RadioModel {
    key: "ftdx10",
    name: "FTDX10",
    maker: Maker::Yaesu,
    family: Family::Yaesu,
    proto: Proto::Text(&SPEC),
    id: "0761",
    hamlib: 1042,
    roles: crate::model::Roles::NONE,
    menu: &[],
    ..FTDX3000
};
