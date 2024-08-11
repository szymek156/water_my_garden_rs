//! Actual logic of watering, agnostic to underlying hardware

use std::{
    collections::HashMap,
    sync::mpsc::{channel, Sender},
};

use chrono::NaiveTime;

use log::{debug, info};

use crate::{
    clock::{ClockServiceChannel, ClockServiceMessage},
    sections::{Section, SectionDuration, SectionsServiceChannel},
};

#[derive(Debug)]
pub enum WateringServiceMessage {
    Alarm1Fired,
    Alarm2Fired,
    StartWateringAt(NaiveTime),
    SetSectionDuration(Section, SectionDuration),
}
pub type WateringServiceChannel = Sender<WateringServiceMessage>;

pub struct WateringService {
    clock_tx: ClockServiceChannel,
    sections_tx: SectionsServiceChannel,
    // TODO: watchdog for section opening
    current_section: Section,
    section_durations: HashMap<Section, SectionDuration>,
}

impl WateringService {
    pub fn new(clock_tx: ClockServiceChannel, sections_tx: SectionsServiceChannel) -> Self {
        Self {
            clock_tx,
            sections_tx,
            current_section: Section::None,
            section_durations: enum_iterator::all::<Section>()
                .map(|section| (section, SectionDuration::default()))
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
        std::thread::spawn(move || {
            log::info!("Hello from Watering service!");

            while let Ok(msg) = rx.recv() {
                self.handle_msg(msg)
            }
        });

        tx
    }

    fn handle_msg(&mut self, msg: WateringServiceMessage) {
        log::debug!("Handling {msg:?}");

        match msg {
            WateringServiceMessage::Alarm1Fired => {
                info!("Got notification about alarm1");

                // start watchdog

                // There should be no watering in progress
                assert_eq!(self.current_section, Section::None);

                self.water_next_section()
            }
            WateringServiceMessage::Alarm2Fired => {
                // This alarm should be assigned to some section
                assert_ne!(self.current_section, Section::None);

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
        info!("Clearing alarm2");
        self.clock_tx
            .send(ClockServiceMessage::DisableAlarm2)
            .unwrap();
    }

    fn set_section_timer(&self, section_duration: &SectionDuration) {
        info!("Arming alarm2 {}", section_duration);
        // Arm alarm2 for that section
        self.clock_tx
            .send(ClockServiceMessage::SetAlarm2After(*section_duration))
            .unwrap();
    }

    fn disable_section(&self, section: Section) {
        info!("Disabling {section:?}");
        self.sections_tx
            .send(crate::sections::SectionsServiceMessage::Disable(section))
            .unwrap();
    }

    fn enable_section(&self, section: Section) {
        info!("Enabling {section:?}");
        self.sections_tx
            .send(crate::sections::SectionsServiceMessage::Enable(section))
            .unwrap();
    }
}

#[cfg(test)]
pub mod tests {
    use crate::sections::SectionsServiceMessage;

    use super::*;

    struct ClockMock;

    impl ClockMock {
        /// rx - is used to handle requests from the service under test
        /// tx - sends captured messages to the test, for verification
        fn start(rx: Receiver<ClockServiceMessage>, tx: ClockServiceChannel) {
            std::thread::spawn(move || {
                let mut now =
                    NaiveDateTime::parse_from_str("2015-09-05 23:56:04", "%Y-%m-%d %H:%M:%S")
                        .unwrap();

                while let Ok(msg) = rx.recv() {
                    tx.send(msg.clone()).unwrap();

                    match msg {
                        ClockServiceMessage::GetDateTime(tx) => {
                            tx.send(now).unwrap();
                        }
                        ClockServiceMessage::SetAlarm2After(offset) => {
                            let future = now.checked_add_signed(offset.into_inner()).unwrap();
                            now = future;
                        }
                        _msg => {}
                    }
                }
            });
        }
    }

    pub fn example_valid_configuration_works() {
        let (clock_tx, rx) = channel();
        let (tx, clock_rx) = channel();
        ClockMock::start(rx, tx);

        let (sections_tx, sections_rx) = channel();

        let mut watering = WateringService::new(clock_tx, sections_tx);

        // Valid clean state
        assert_eq!(watering.current_section, Section::None);

        let vegs_duration = TimeDelta::minutes(5).try_into().unwrap();
        let flowers_duration = TimeDelta::minutes(10).try_into().unwrap();
        let grass_duration = TimeDelta::minutes(20).try_into().unwrap();
        let terrace_duration = TimeDelta::minutes(8).try_into().unwrap();

        watering.section_durations = [
            (Section::Vegs, vegs_duration),
            (Section::Flowers, flowers_duration),
            (Section::Grass, grass_duration),
            (Section::Terrace, terrace_duration),
        ]
        .into();

        // Simulate interrupt from the clock - watering should start
        watering.handle_msg(WateringServiceMessage::Alarm1Fired);

        // Expect Vegs to be first
        verify_moved_to_next_section(
            Section::None,
            watering.current_section,
            Section::Vegs,
            vegs_duration,
            &sections_rx,
            &clock_rx,
        );

        // Simulate vegs finished
        watering.handle_msg(WateringServiceMessage::Alarm2Fired);

        verify_moved_to_next_section(
            Section::Vegs,
            watering.current_section,
            Section::Flowers,
            flowers_duration,
            &sections_rx,
            &clock_rx,
        );

        // Simulate flowers finished
        watering.handle_msg(WateringServiceMessage::Alarm2Fired);
        verify_moved_to_next_section(
            Section::Flowers,
            watering.current_section,
            Section::Grass,
            grass_duration,
            &sections_rx,
            &clock_rx,
        );

        // Simulate grass finished
        watering.handle_msg(WateringServiceMessage::Alarm2Fired);
        verify_moved_to_next_section(
            Section::Grass,
            watering.current_section,
            Section::Terrace,
            terrace_duration,
            &sections_rx,
            &clock_rx,
        );

        // Simulate terrace finished
        watering.handle_msg(WateringServiceMessage::Alarm2Fired);

        // Expect watering moved to None section
        assert_eq!(watering.current_section, Section::None);

        // Expect Terrace section got disabled
        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Disable(Section::Terrace)
        ));

        // Expect alarm2 is disabled
        assert!(matches!(
            clock_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            ClockServiceMessage::DisableAlarm2
        ));
    }

    pub fn can_skip_a_section() {
        let (clock_tx, rx) = channel();
        let (tx, clock_rx) = channel();
        ClockMock::start(rx, tx);

        let (sections_tx, sections_rx) = channel();

        let mut watering = WateringService::new(clock_tx, sections_tx);

        // Valid clean state
        assert_eq!(watering.current_section, Section::None);

        // Skip flowers and terrace
        let vegs_duration = TimeDelta::minutes(5).try_into().unwrap();
        let grass_duration = TimeDelta::minutes(20).try_into().unwrap();
        watering.section_durations = [
            (Section::Vegs, vegs_duration),
            (Section::Flowers, SectionDuration::default()),
            (Section::Grass, grass_duration),
            (Section::Terrace, SectionDuration::default()),
        ]
        .into();

        // Simulate interrupt from the clock - watering should start
        watering.handle_msg(WateringServiceMessage::Alarm1Fired);

        // Expect Vegs to be first
        verify_moved_to_next_section(
            Section::None,
            watering.current_section,
            Section::Vegs,
            vegs_duration,
            &sections_rx,
            &clock_rx,
        );

        // Simulate vegs finished, should skip flowers, go to grass
        watering.handle_msg(WateringServiceMessage::Alarm2Fired);

        // Expect watering moved to next valid section - grass
        assert_eq!(watering.current_section, Section::Grass);

        // Expect vegs section got disabled
        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Disable(Section::Vegs)
        ));

        // Expect skipped flowers section got disabled
        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Disable(Section::Flowers)
        ));

        // Expect grass section got enabled
        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Enable(Section::Grass)
        ));

        // Expect alarm2 is set
        match clock_rx.recv_timeout(Duration::from_secs(1)).unwrap() {
            ClockServiceMessage::SetAlarm2After(offset) => {
                assert_eq!(offset, grass_duration);
            }
            _ => panic!("Unexpected message"),
        }

        // Simulate grass finished, skip terrace, finish watering
        watering.handle_msg(WateringServiceMessage::Alarm2Fired);

        // Expect watering moved to None section
        assert_eq!(watering.current_section, Section::None);

        // Expect Grass section got disabled
        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Disable(Section::Grass)
        ));
        // Expect skipped Terrace section got disabled
        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Disable(Section::Terrace)
        ));

        // Expect alarm2 is disabled
        assert!(matches!(
            clock_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            ClockServiceMessage::DisableAlarm2
        ));
    }

    pub fn can_skip_all_sections() {
        let (clock_tx, rx) = channel();
        let (tx, clock_rx) = channel();
        ClockMock::start(rx, tx);

        let (sections_tx, sections_rx) = channel();

        let mut watering = WateringService::new(clock_tx, sections_tx);

        // Valid clean state
        assert_eq!(watering.current_section, Section::None);

        // Skip them all!
        watering.section_durations = [
            (Section::Vegs, SectionDuration::default()),
            (Section::Flowers, SectionDuration::default()),
            (Section::Grass, SectionDuration::default()),
            (Section::Terrace, SectionDuration::default()),
        ]
        .into();

        // Simulate interrupt from the clock - watering should start
        watering.handle_msg(WateringServiceMessage::Alarm1Fired);

        // Expect none of the sections triggered
        // Expect watering moved to next valid section - None
        assert_eq!(watering.current_section, Section::None);

        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Disable(Section::None)
        ));
        // Expect skipped vegs section got disabled
        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Disable(Section::Vegs)
        ));

        // Expect skipped flowers section got disabled
        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Disable(Section::Flowers)
        ));

        // Expect skipped grass section got disabled
        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Disable(Section::Grass)
        ));

        // Expect skipped terrace section got disabled
        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Disable(Section::Terrace)
        ));

        // Expect alarm2 is disabled
        assert!(matches!(
            clock_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            ClockServiceMessage::DisableAlarm2
        ));
    }

    fn verify_moved_to_next_section(
        _current_section: Section,
        next_section: Section,
        expected_next_section: Section,
        expected_duration: SectionDuration,
        sections_rx: &Receiver<SectionsServiceMessage>,
        clock_rx: &Receiver<ClockServiceMessage>,
    ) {
        // Expect watering moved to valid section
        assert_eq!(next_section, expected_next_section);

        // Expect current section got disabled
        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Disable(_current_section)
        ));

        // Expect next section got enabled
        assert!(matches!(
            sections_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            SectionsServiceMessage::Enable(_expected_next_section)
        ));

        // Expect alarm2 is set
        match clock_rx.recv_timeout(Duration::from_secs(1)).unwrap() {
            ClockServiceMessage::SetAlarm2After(offset) => {
                assert_eq!(expected_duration, offset)
            }
            _ => panic!("Unexpected message"),
        }
    }
}
