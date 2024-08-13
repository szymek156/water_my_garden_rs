//! Abstraction over timekeeping hardware

use anyhow::{anyhow, Result};
use chrono::{NaiveDateTime, NaiveTime};
use ds323x::{ic::DS3231, interface::I2cInterface, DateTimeAccess, Ds323x};
use esp_idf_svc::hal::{
    delay,
    gpio::{IOPin, Input, PinDriver},
    i2c::{I2c, I2cConfig, I2cDriver},
    peripheral::Peripheral,
    prelude::*,
    task::queue::Queue,
};
use log::{error, info};
use std::sync::mpsc::{Receiver, Sender};

use crate::{sections::SectionDuration, watering::WateringServiceMessage};

pub struct ClockService<IntGPIO: IOPin> {
    rtc: Ds323x<I2cInterface<I2cDriver<'static>>, DS3231>,
    int_pin: PinDriver<'static, IntGPIO, Input>,
    section_alarm_subscribers: Vec<Sender<WateringServiceMessage>>,
    watering_alarm_subscribers: Vec<Sender<WateringServiceMessage>>,
}

#[derive(Debug)]
pub struct ClockStatus {
    // alarm1 when
    // alarm2 when
    // status reg
    // other regs
    temp: f32,
    now: NaiveDateTime,
}

#[derive(Debug, Clone)]
pub enum ClockServiceMessage {
    InterruptArrived(u32),
    /// Use alarm1 for section notification - it has H:M:S resolution, good for requests like "water section for 10 minutes from now"
    SubscribeForSectionAlarm(Sender<WateringServiceMessage>),
    /// Use alarm2 for watering notification - it has H:M resolution, it's enough for "set watering on 20:30"
    SubscribeForWateringAlarm(Sender<WateringServiceMessage>),
    /// Set section alarm in some time in the future starting from now. E.g. in 15 minutes
    SetSectionAlarmAfter(SectionDuration),
    /// Set watering to exact time HH:MM::00
    SetWateringAlarmAt(NaiveTime),
    DisableSectionAlarm,
    DisableWateringAlarm,
    GetStatus(Sender<ClockStatus>),
    GetDateTime(Sender<NaiveDateTime>),
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

        Ok(Self {
            rtc,
            int_pin,
            section_alarm_subscribers: vec![],
            watering_alarm_subscribers: vec![],
        })
    }

    /// Starts the Clock Service, returns the ClockServiceChannel to communicate with it
    pub fn start(mut self) -> ClockServiceChannel {
        // Create channel that is used to communicate with this service
        let (tx, rx) = std::sync::mpsc::channel();

        self.start_interrupt_service(tx.clone());

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

                    // Try to send to the subscribers, if fails, it means rx "unsubscribed", filter such entries
                    if self.rtc.has_alarm1_matched().unwrap() {
                        let subscribers = self
                            .section_alarm_subscribers
                            .into_iter()
                            .filter(|tx| tx.send(WateringServiceMessage::SectionAlarmFired).is_ok())
                            .collect();
                        self.section_alarm_subscribers = subscribers;
                    }

                    if self.rtc.has_alarm2_matched().unwrap() {
                        let subscribers = self
                            .watering_alarm_subscribers
                            .into_iter()
                            .filter(|tx| tx.send(WateringServiceMessage::WateringAlarmFired).is_ok())
                            .collect();
                        self.watering_alarm_subscribers = subscribers;
                    }

                    self.enable_interrupt();
                }
                ClockServiceMessage::SubscribeForSectionAlarm(tx) => self.section_alarm_subscribers.push(tx),
                ClockServiceMessage::SubscribeForWateringAlarm(tx) => self.watering_alarm_subscribers.push(tx),
                ClockServiceMessage::SetSectionAlarmAfter(offset) => {
                    info!("Handling Alarm1 - Section with offset {offset}");
                    let now = self.get_current_datetime().unwrap();
                    let future = now.checked_add_signed(offset.into_inner()).unwrap();
                    info!("Setting Alarm1 - Section to {future}");
                    self.rtc.set_alarm1_hms(future.time()).unwrap();
                    self.rtc.enable_alarm1_interrupts().unwrap();
                }
                ClockServiceMessage::SetWateringAlarmAt(when) => {
                    info!("Setting Alarm2 - Watering to {when}");
                    self.rtc.set_alarm2_hm(when).unwrap();
                    self.rtc.enable_alarm2_interrupts().unwrap();
                }
                ClockServiceMessage::DisableSectionAlarm => {
                    info!("Disabling Alarm1 - Section");
                    self.rtc.disable_alarm1_interrupts().unwrap();
                }
                ClockServiceMessage::DisableWateringAlarm => {
                    info!("Disabling Alarm2 - Watering");
                    self.rtc.disable_alarm2_interrupts().unwrap();
                }
                ClockServiceMessage::GetStatus(tx) => {
                    let temp = self.get_temperature().unwrap();
                    let now = self.get_current_datetime().unwrap();

                    let status = ClockStatus { temp, now };

                    info!("Reporting Clock status {status:#?}");
                    if let Err(e) = tx.send(status) {
                        error!("Failed to send Clock status as a response {e}");
                    }
                }
                ClockServiceMessage::GetDateTime(tx) => {
                    let now = self.get_current_datetime().unwrap();

                    info!("Reporting time {now}");
                    if let Err(e) = tx.send(now) {
                        error!("Failed to send time status as a response {e}");
                    }
                }
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

        info!("RTC: {datetime}");
        Ok(datetime)
    }

    fn get_temperature(&mut self) -> Result<f32> {
        let temp = self
            .rtc
            .temperature()
            .map_err(|e| anyhow!("Failed to read temperature {e:?}"))?;
        Ok(temp)
    }
}
