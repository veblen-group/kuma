use binance::model::BookTickerEvent;

#[derive(Debug)]
pub(super) struct BookTicker {}

impl BookTicker {
    // from websocket?
    pub fn from_binance_websocket_event(_raw: BookTickerEvent) -> Self {
        Self {}
    }
}
