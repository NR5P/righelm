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

pub static K3: RadioModel = RadioModel {
    key: "k3",
    name: "K3",
    maker: Maker::Elecraft,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "017",
    hamlib: 2029,
    roles: crate::model::Roles::NONE,
    menu: &[],
    ..FTDX3000
};

pub static KX3: RadioModel = RadioModel {
    key: "kx3",
    name: "KX3",
    maker: Maker::Elecraft,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "017",
    hamlib: 2045,
    roles: crate::model::Roles::NONE,
    menu: &[],
    ..FTDX3000
};

pub static KX2: RadioModel = RadioModel {
    key: "kx2",
    name: "KX2",
    maker: Maker::Elecraft,
    family: Family::Kenwood,
    proto: Proto::Text(&SPEC),
    id: "017",
    hamlib: 2044,
    roles: crate::model::Roles::NONE,
    menu: &[],
    ..FTDX3000
};
