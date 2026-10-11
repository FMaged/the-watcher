// One-off probe: does the parser match what the endpoint returns today?
use watcher_core::{PriceSource, Ticker, TradingDay};
use watcher_feeds::YahooPriceSource;

#[tokio::main]
async fn main() {
    let src = YahooPriceSource::new().expect("client");
    let ticker: Ticker = "AAPL".parse().unwrap();
    let from = TradingDay::from_ymd(2026, 9, 28).unwrap();
    let to = TradingDay::from_ymd(2026, 10, 9).unwrap();

    match src.daily_bars(&ticker, from, to).await {
        Ok(f) => {
            println!("bars: {}  skipped: {}", f.bars.len(), f.skipped);
            for b in f.bars.iter().take(3) {
                println!(
                    "  {} o={} h={} l={} c={} v={}",
                    b.day, b.open, b.high, b.low, b.close, b.volume
                );
            }
            for b in &f.bars {
                assert!(b.is_coherent(), "incoherent bar got through: {b:?}");
            }
        }
        Err(e) => println!("fetch failed: {e}  (retryable: {})", e.is_retryable()),
    }

    match src.daily_bars(&"ZZZZZ".parse().unwrap(), from, to).await {
        Ok(f) => println!(
            "unknown symbol gave {} bars (expected an error)",
            f.bars.len()
        ),
        Err(e) => println!("unknown symbol -> {e}"),
    }
}
