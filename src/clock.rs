use std::num::NonZeroU32;

use anyhow::{anyhow, Result};
use chrono::{NaiveDateTime, TimeDelta};
use ds323x::{ic::DS3231, interface::I2cInterface, DateTimeAccess, Ds323x};
use esp_idf_svc::hal::{
    gpio::{IOPin, Input, PinDriver},
    i2c::{I2c, I2cConfig, I2cDriver},
    peripheral::Peripheral,
    prelude::*,
    task::notification::Notification,
};

pub struct RTCClock<IntGPIO: IOPin> {
    rtc: Ds323x<I2cInterface<I2cDriver<'static>>, DS3231>,
    int_pin: PinDriver<'static, IntGPIO, Input>,
}

impl<IntGPIO: IOPin> RTCClock<IntGPIO> {
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

        rtc.disable_32khz_output()
            .map_err(|e| anyhow!("Cannot disable 32khz output {e:?}"))?;

        rtc.disable_square_wave()
            .map_err(|e| anyhow!("Cannot disable square wave {e:?}"))?;

        rtc.use_int_sqw_output_as_interrupt()
            .map_err(|e| anyhow!("Cannot set sqw as interrupt {e:?}"))?;

        rtc.disable_alarm1_interrupts()
            .map_err(|e| anyhow!("Cannot disable alarm1 INT {e:?}"))?;
        rtc.disable_alarm2_interrupts()
            .map_err(|e| anyhow!("Cannot disable alarm2 INT {e:?}"))?;

        // Configure INT GPIO, the SQW output pin of the RTC is connected to it
        let mut int_pin = PinDriver::input(int_pin).unwrap();

        // INT pin on RTC is high by default, listen on falling edge
        int_pin
            .set_interrupt_type(esp_idf_svc::hal::gpio::InterruptType::NegEdge)
            .unwrap();
        int_pin
            .set_pull(esp_idf_svc::hal::gpio::Pull::Down)
            .unwrap();

        Ok(Self { rtc, int_pin })
    }

    pub fn start(mut self) {
        // This is a thin wrapper over the FreeRTOS task
        std::thread::spawn(move || {
            log::info!("Hello from Clock service!");
            // Setup INT handler, communicate from ISR to task using Notification
            let notification = Notification::new();
            let notifier = notification.notifier();

            unsafe {
                self.int_pin
                    .subscribe(move || {
                        static mut INT_COUNT: u32 = 1;

                        notifier.notify_and_yield(NonZeroU32::new(INT_COUNT).unwrap());

                        INT_COUNT += 1;
                    })
                    .unwrap()
            };

            self.int_pin.enable_interrupt().unwrap();
            self.rtc.clear_alarm1_matched_flag().unwrap();
            self.rtc.clear_alarm2_matched_flag().unwrap();

            self.test_arming_alarm();

            while let Some(msg) = notification.wait(esp_idf_svc::hal::delay::BLOCK) {
                log::info!("Got interrupt notification! #{msg}");

                // Clear the flag on RTC indicating the interrupt got handled, it will enable RTC to trigger again.
                if self.rtc.has_alarm1_matched().unwrap() {
                    self.test_arming_alarm();
                    self.rtc.clear_alarm1_matched_flag().unwrap();
                }

                self.int_pin.enable_interrupt().unwrap();
            }
        });
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
