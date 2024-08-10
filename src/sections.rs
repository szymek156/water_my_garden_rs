//! Abstraction over hardware for enabling/disabling sections

use std::sync::mpsc::{Receiver, Sender};

use anyhow::Result;
use enum_iterator::Sequence;
use esp_idf_svc::hal::{
    gpio::{Output, OutputPin, PinDriver},
    peripheral::Peripheral,
};

#[derive(Debug, PartialEq, Sequence, Hash, Eq, Copy, Clone)]
pub enum Section {
    Vegs,
    Flowers,
    Grass,
    Terrace,
    None,
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
    pub fn start(mut self) -> SectionsServiceChannel {
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
