use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;
use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    #[serde(default)]
    pub cached_input_tokens: u32,
}

#[derive(Debug, Clone)]
pub enum Event {
    TextDelta(String),
    OutputItem(serde_json::Value),
    Done { usage: Usage },
}

pub struct StreamResponse {
    inner: Pin<Box<dyn Stream<Item = Result<Event, Error>> + Send>>,
}

impl StreamResponse {
    pub fn new(stream: impl Stream<Item = Result<Event, Error>> + Send + 'static) -> Self {
        Self {
            inner: Box::pin(stream),
        }
    }

    pub fn from_events(events: Vec<Event>) -> Self {
        Self::new(futures::stream::iter(events.into_iter().map(Ok)))
    }
}

impl Stream for StreamResponse {
    type Item = Result<Event, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
