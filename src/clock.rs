use std::sync::mpsc::{Receiver, Sender};

use anyhow::{anyhow, Result};
use chrono::{NaiveDateTime, TimeDelta};
use ds323x::{ic::DS3231, interface::I2cInterface, DateTimeAccess, Ds323x};
use esp_idf_svc::hal::{
    delay,
    gpio::{IOPin, Input, PinDriver},
    i2c::{I2c, I2cConfig, I2cDriver},
    peripheral::Peripheral,
    prelude::*,
    task::queue::Queue,
};

pub struct ClockService<IntGPIO: IOPin> {
    rtc: Ds323x<I2cInterface<I2cDriver<'static>>, DS3231>,
    int_pin: PinDriver<'static, IntGPIO, Input>,
}

pub enum ClockServiceMessage {
    InterruptArrived(u32),
    SubscribeForAlarm1(Sender<()>),
}
pub type ClockServiceChannel = Sender<ClockServiceMessage>;

impl<IntGPIO: IOPin> ClockService<IntGPIO> {
    pub fn new(
        sda_pin: impl Peripheral<P = impl IOPin> + 'static,
        scl_pin: impl Peripheral<P = impl IOPin> + 'static,
        int_pin: impl Peripheral<P = IntGPIO> + 'static,
        i2c: impl Peripheral<P = impl I2c> + 'static,
    ) -> Result<Self> {
        // Configure RTC I2C driver
        let config = I2cConfig::new().baudrate(400.kHz().into());
        let i2c_dev = I2cDriver::new(i2c, sda_pin, scl_pin, &config)?;

        let mut rtc = Ds323x::new_ds3231(i2c_dev);

        // This pin is unused
        rtc.disable_32khz_output()
            .map_err(|e| anyhow!("Cannot disable 32khz output {e:?}"))?;

        rtc.use_int_sqw_output_as_interrupt()
            .map_err(|e| anyhow!("Cannot set sqw as interrupt {e:?}"))?;

        // Cleanup state from previous reboot
        rtc.disable_alarm1_interrupts()
            .map_err(|e| anyhow!("Cannot disable alarm1 INT {e:?}"))?;
        rtc.disable_alarm2_interrupts()
            .map_err(|e| anyhow!("Cannot disable alarm2 INT {e:?}"))?;

        // Configure INT GPIO, the SQW output pin of the RTC is connected to it
        let mut int_pin = PinDriver::input(int_pin).unwrap();

        int_pin
            .set_pull(esp_idf_svc::hal::gpio::Pull::Down)
            .unwrap();

        Ok(Self { rtc, int_pin })
    }

    /// Starts the Clock Service, returns the ClockServiceChannel to communicate with it
    pub fn start(mut self) -> ClockServiceChannel {
        // Create channel that is used to communicate with this service
        let (tx, rx) = std::sync::mpsc::channel();

        self.start_interrupt_service(tx.clone());

        self.test_arming_alarm();

        // Create Clock service
        std::thread::spawn(move || self.clock_service(rx));

        tx
    }

    fn clock_service(mut self, rx: Receiver<ClockServiceMessage>) {
        log::info!("Hello from Clock service!");

        while let Ok(msg) = rx.recv() {
            match msg {
                ClockServiceMessage::InterruptArrived(int_count) => {
                    log::info!("Got interrupt notification in service! #{int_count}");

                    self.test_arming_alarm();

                    self.enable_interrupt();
                }
                ClockServiceMessage::SubscribeForAlarm1(_tx) => todo!(),
            }
        }
    }

    /// Setup RTC interrupt handling: ISR -> interrupt-handler task -> ClockService task
    fn start_interrupt_service(&mut self, tx: ClockServiceChannel) {
        // Communicate from ISR with task using FreeRTOS queue.
        // Alternative is to use Notification - this one is however bounded to the task,
        // and cannot be moved across threads.

        // ISR part, will use it to push back notifications
        let queue_isr = Queue::new(10);

        // Thread part, will pop front notifications
        // SAFETY: Owner of this queue is ISR, captured in a closure. Will never drop.
        let queue_thread = unsafe { Queue::<u32>::new_borrowed(queue_isr.as_raw()) };

        // INT pin on RTC is high by default, listen on falling edge
        self.int_pin
            .set_interrupt_type(esp_idf_svc::hal::gpio::InterruptType::NegEdge)
            .unwrap();

        // Start listening on interrupt, set ISR that pushes interrupt notifications to the queue
        // SAFETY: Using ISR-safe calls here
        unsafe {
            self.int_pin
                .subscribe(move || {
                    static mut INT_COUNT: u32 = 1;

                    let high_prio_task_was_awoken = queue_isr
                        .send_back(INT_COUNT, delay::NON_BLOCK)
                        .expect("The interrupt queue is full!");
                    INT_COUNT += 1;

                    if high_prio_task_was_awoken {
                        // This is FreeRTOS detail:
                        // Context switch should be performed before the interrupt is exited. This will ensure that the
                        // interrupt returns directly to the highest priority Ready state task
                        esp_idf_svc::hal::task::do_yield();
                    }
                })
                .unwrap()
        };

        self.enable_interrupt();

        // Create interrupt-handler task, will communicate with the Clock service when interrupt arrive
        // This is a thin wrapper over the FreeRTOS task
        // TODO: that thread might be redundant if embassy channels are safe to call from ISR.
        // The std::sync::mpsc channels are not. Therefore current solution uses ISR-safe primitive (FreeRTOS queue)
        // to communicate with following thread, and this thread finally communicates with the Clock service using mpsc channel.
        std::thread::spawn(move || {
            log::info!("Hello from RTC interrupt task!");

            // Receive interrupt from ISR
            while let Some((int_count, _)) = queue_thread.recv_front(delay::BLOCK) {
                log::debug!("Got interrupt notification! #{int_count}");
                // Pass it to the service
                tx.send(ClockServiceMessage::InterruptArrived(int_count))
                    .expect("Cannot notify Clock service");
            }
        });
    }

    fn enable_interrupt(&mut self) {
        // Clear the flag on RTC indicating the interrupt got handled, it will enable RTC to trigger again.
        if self.rtc.has_alarm1_matched().unwrap() {
            self.rtc.clear_alarm1_matched_flag().unwrap();
        }

        if self.rtc.has_alarm2_matched().unwrap() {
            self.rtc.clear_alarm2_matched_flag().unwrap();
        }

        // GPIO interrupt got disabled after fire, re-enable again
        self.int_pin.enable_interrupt().unwrap();
    }

    fn get_current_datetime(&mut self) -> Result<NaiveDateTime> {
        // TODO: since wifi is connected, NTP for the clock
        let datetime = self
            .rtc
            .datetime()
            .map_err(|e| anyhow!("Failed to read current datetime {e:?}"))?;

        Ok(datetime)
    }

    fn get_temperature(&mut self) -> Result<f32> {
        let temp = self
            .rtc
            .temperature()
            .map_err(|e| anyhow!("Failed to read temperature {e:?}"))?;
        Ok(temp)
    }

    fn test_arming_alarm(&mut self) {
        let now = self.get_current_datetime().unwrap();
        let future = now
            .checked_add_signed(TimeDelta::new(5, 0).unwrap())
            .unwrap();

        log::info!(
            "setting alarm {} -> {future}",
            self.get_current_datetime().unwrap()
        );

        self.rtc.set_alarm1_hms(future.time()).unwrap();
        self.rtc.enable_alarm1_interrupts().unwrap();
    }
}
