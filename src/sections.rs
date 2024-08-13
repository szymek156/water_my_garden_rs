//! Abstraction over hardware for enabling/disabling sections

use std::{
    fmt::Display,
    str::FromStr,
    sync::mpsc::{Receiver, Sender},
};

use anyhow::{bail, Result};
use chrono::TimeDelta;
use enum_iterator::Sequence;
use esp_idf_svc::hal::{
    gpio::{Output, OutputPin, PinDriver},
    peripheral::Peripheral,
};
use serde::{Deserialize, Deserializer};

#[derive(Deserialize, Debug, PartialEq, Sequence, Hash, Eq, Copy, Clone)]
pub enum Section {
    Vegs,
    Flowers,
    Grass,
    Terrace,
    None,
}

// TODO: tests
/// Newtype that gets reasonable values for section watering duration - non negative and less than 2 hours
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SectionDuration(TimeDelta);

/// Lets assume valid format is minutes, like "90" is 1 hour 30 mins
impl<'de> Deserialize<'de> for SectionDuration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let minutes = i64::deserialize(deserializer)?;
        let duration =
            SectionDuration::new(TimeDelta::minutes(minutes)).map_err(serde::de::Error::custom)?;

        Ok(duration)
    }
}

impl SectionDuration {
    pub fn new(td: TimeDelta) -> Result<Self> {
        if td.num_seconds() < 0 {
            bail!("delta is negative");
        }

        if td.num_hours() > 1 {
            bail!("cannot water section for more than 2 hours");
        }

        Ok(Self(td))
    }

    pub fn into_inner(self) -> TimeDelta {
        self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl TryInto<SectionDuration> for TimeDelta {
    type Error = anyhow::Error;

    fn try_into(self) -> std::result::Result<SectionDuration, Self::Error> {
        SectionDuration::new(self)
    }
}

impl Display for SectionDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:02}h {:02}m {:02}s",
            self.0.num_hours(),
            self.0.num_minutes() % 60,
            self.0.num_seconds() % 60
        )
    }
}

pub enum SectionsServiceMessage {
    Enable(Section),
    Disable(Section),
}
pub type SectionsServiceChannel = Sender<SectionsServiceMessage>;

pub struct Sections<
    VegsGPIO: OutputPin,
    TerraceGPIO: OutputPin,
    FlowersGPIO: OutputPin,
    GrassGPIO: OutputPin,
> {
    vegs: PinDriver<'static, VegsGPIO, Output>,
    terrace: PinDriver<'static, TerraceGPIO, Output>,
    flowers: PinDriver<'static, FlowersGPIO, Output>,
    grass: PinDriver<'static, GrassGPIO, Output>,
}

impl<VegsGPIO: OutputPin, TerraceGPIO: OutputPin, FlowersGPIO: OutputPin, GrassGPIO: OutputPin>
    Sections<VegsGPIO, TerraceGPIO, FlowersGPIO, GrassGPIO>
{
    pub fn new(
        vegs_pin: impl Peripheral<P = VegsGPIO> + 'static,
        terrace_pin: impl Peripheral<P = TerraceGPIO> + 'static,
        flowers_pin: impl Peripheral<P = FlowersGPIO> + 'static,
        grass_pin: impl Peripheral<P = GrassGPIO> + 'static,
    ) -> Result<Self> {
        let mut sections = Self {
            vegs: PinDriver::output(vegs_pin)?,
            terrace: PinDriver::output(terrace_pin)?,
            flowers: PinDriver::output(flowers_pin)?,
            grass: PinDriver::output(grass_pin)?,
        };

        sections.vegs.set_low()?;
        sections.flowers.set_low()?;
        sections.terrace.set_low()?;
        sections.grass.set_low()?;

        Ok(sections)
    }

    /// Starts the Sections Service, returns the SectionsServiceChannel to communicate with it
    pub fn start(self) -> SectionsServiceChannel {
        // Create channel that is used to communicate with this service
        let (tx, rx) = std::sync::mpsc::channel();

        // Create Sections service
        std::thread::spawn(move || self.sections_service(rx));

        tx
    }

    fn sections_service(mut self, rx: Receiver<SectionsServiceMessage>) {
        log::info!("Hello from Sections service!");

        while let Ok(msg) = rx.recv() {
            match msg {
                SectionsServiceMessage::Enable(section) => {
                    log::info!("Enabling {section:?}");
                    match section {
                        Section::Vegs => self.vegs.set_high(),
                        Section::Flowers => self.flowers.set_high(),
                        Section::Grass => self.grass.set_high(),
                        Section::Terrace => self.terrace.set_high(),
                        Section::None => Ok(()),
                    }
                    .unwrap();
                }
                SectionsServiceMessage::Disable(section) => {
                    log::info!("Disabling {section:?}");
                    match section {
                        Section::Vegs => self.vegs.set_low(),
                        Section::Flowers => self.flowers.set_low(),
                        Section::Grass => self.grass.set_low(),
                        Section::Terrace => self.terrace.set_low(),
                        Section::None => Ok(()),
                    }
                    .unwrap();
                }
            }
        }
    }
}
