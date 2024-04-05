use embassy_rp::gpio;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};

use crate::LedResources;

pub type LedIndications = Signal<CriticalSectionRawMutex, ()>;

#[embassy_executor::task]
pub async fn run(res: LedResources, signal: &'static LedIndications) {
    let mut led = gpio::Output::new(res.green, gpio::Level::Low);

    loop {
        signal.wait().await;

        led.set_high();
        Timer::after(Duration::from_millis(20)).await;
        led.set_low();
    }
}
