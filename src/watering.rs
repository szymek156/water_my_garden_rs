//! Actual logic of watering, agnostic to underlying hardware

use std::{
    collections::HashMap,
    sync::mpsc::{channel, Receiver, Sender},
    time::Duration,
};

use chrono::{NaiveDateTime, NaiveTime, TimeDelta};
use enum_iterator::{all, cardinality, first, last, next, previous, reverse_all, Sequence};
use log::{debug, info};

use crate::{
    clock::{ClockServiceChannel, ClockServiceMessage},
    sections::{Section, SectionsServiceChannel},
};

pub enum WateringServiceMessage {
    Alarm1Fired,
    Alarm2Fired,
    StartWateringAt(NaiveTime),
    SetSectionDuration(Section, TimeDelta),
}
pub type WateringServiceChannel = Sender<WateringServiceMessage>;

pub struct WateringService {
    clock_tx: ClockServiceChannel,
    sections_tx: SectionsServiceChannel,
    // TODO: watchdog for section opening
    current_section: Section,
    section_durations: HashMap<Section, TimeDelta>,
}

impl WateringService {
    pub fn new(clock_tx: ClockServiceChannel, sections_tx: SectionsServiceChannel) -> Self {
        Self {
            clock_tx,
            sections_tx,
            current_section: Section::None,
            section_durations: enum_iterator::all::<Section>()
                .map(|section| (section, TimeDelta::zero()))
                .collect::<HashMap<_, _>>(),
        }
    }

    /// Starts the Watering Service, returns the WateringServiceChannel to communicate with it
    pub fn start(mut self) -> WateringServiceChannel {
        // Create channel that is used to communicate with this service
        let (tx, rx) = channel();

        self.clock_tx
            .send(ClockServiceMessage::SubscribeForAlarm1(tx.clone()))
            .unwrap();

        // Create Watering service
        std::thread::spawn(move || self.watering_service(rx));

        tx
    }

    fn watering_service(mut self, rx: Receiver<WateringServiceMessage>) {
        log::info!("Hello from Watering service!");

        while let Ok(msg) = rx.recv() {
            match msg {
                WateringServiceMessage::Alarm1Fired => {
                    info!("Got notification about alarm1");

                    // start watchdog

                    // There should be no watering in progress
                    assert_eq!(self.current_section, Section::None);

                    self.water_next_section()
                }
                WateringServiceMessage::Alarm2Fired => {
                    info!("Got notification about alarm2");
                    self.water_next_section()
                }
                WateringServiceMessage::StartWateringAt(when) => {
                    info!("Setting up watering on {when}");
                    self.clock_tx
                        .send(ClockServiceMessage::SetAlarm1(when))
                        .unwrap();
                }
                WateringServiceMessage::SetSectionDuration(section, duration) => {
                    info!("Setting up section {section:?} for {duration}");
                    let _ = self.section_durations.insert(section, duration);
                }
            }
        }
    }

    fn water_next_section(&mut self) {
        debug!("Disabling {:?}", self.current_section);

        // disable current section
        self.disable_section(self.current_section);
        // feed watchdog

        self.current_section = enum_iterator::next_cycle(&self.current_section);

        if self.current_section == Section::None {
            info!("Watering complete");
            // disable alarm2
            self.disable_section_timer();
            // disable watchdog
            return;
        }

        let section_duration = self.section_durations.get(&self.current_section).unwrap();

        if section_duration.is_zero() {
            info!(
                "Section {:?} is disabled, moving to another",
                self.current_section
            );

            self.water_next_section();
            return;
        }

        self.enable_section(self.current_section);
        self.set_section_timer(section_duration);
        // reload watchdog
    }

    fn disable_section_timer(&self) {
        self.clock_tx
            .send(ClockServiceMessage::DisableAlarm2)
            .unwrap();
    }

    fn set_section_timer(&self, section_duration: &TimeDelta) {
        let now = self.get_current_datetime();
        let future = now.checked_add_signed(*section_duration).unwrap();

        // Arm alarm2 for that section
        self.clock_tx
            .send(ClockServiceMessage::SetAlarm2(future.time()))
            .unwrap();
    }

    fn disable_section(&self, section: Section) {
        self.sections_tx
            .send(crate::sections::SectionsServiceMessage::Disable(section))
            .unwrap();
    }

    fn enable_section(&self, section: Section) {
        self.sections_tx
            .send(crate::sections::SectionsServiceMessage::Enable(section))
            .unwrap();
    }

    fn get_current_datetime(&self) -> NaiveDateTime {
        let (tx, mut rx) = channel();
        self.clock_tx
            .send(ClockServiceMessage::GetDateTime(tx))
            .unwrap();
        let mut now = rx.recv().unwrap();
        now
    }
}
