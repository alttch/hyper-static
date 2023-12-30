#![doc = include_str!("../README.md")]
use hyper::{body::Body, Response};
use std::collections::VecDeque;
use std::pin::Pin;

pub mod serve;
pub mod streamer;

pub type Streamed =
    Response<Pin<Box<dyn Body<Data = VecDeque<u8>, Error = ErrorBoxed> + 'static + Send>>>;
pub type ErrorBoxed = Box<dyn std::error::Error + Send + Sync + 'static>;
