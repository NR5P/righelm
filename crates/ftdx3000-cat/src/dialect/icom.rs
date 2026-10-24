//! Icom CI-V translator (being written).

use super::{Back, Dialect, Emulator, Out, Request};
use crate::model::RadioModel;

/// What differs between Icom radios.
#[derive(Debug)]
pub struct CivSpec {}

pub struct Icom {
    model: &'static RadioModel,
    split: super::TextSplitter,
}

impl Icom {
    pub fn new(model: &'static RadioModel, _spec: &'static CivSpec) -> Self {
        Self { model, split: Default::default() }
    }
}

impl Dialect for Icom {
    fn command(&mut self, _frame: &str) -> Vec<Out> {
        let _ = self.model;
        vec![Out::reject()]
    }

    fn split(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.split.split(bytes)
    }

    fn answer(&mut self, _frame: &[u8]) -> Back {
        Back::default()
    }
}

pub struct IcomEmulator {
    model: &'static RadioModel,
    split: super::TextSplitter,
}

impl IcomEmulator {
    pub fn new(model: &'static RadioModel, _spec: &'static CivSpec) -> Self {
        Self { model, split: Default::default() }
    }
}

impl Emulator for IcomEmulator {
    fn split(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.split.split(bytes)
    }

    fn request(&mut self, _frame: &[u8]) -> Request {
        let _ = self.model;
        Request { immediate: Some(b"?;".to_vec()), ..Default::default() }
    }

    fn respond(&mut self, _request: &Request, _answers: &[Option<String>]) -> Vec<u8> {
        Vec::new()
    }

    fn unsolicited(&mut self, _frame: &str) -> Option<Vec<u8>> {
        None
    }
}
