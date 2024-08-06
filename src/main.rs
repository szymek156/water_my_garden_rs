mod clock;
mod wifi;

use anyhow::Result;

use clock::RTCClock;

use enum_iterator::Sequence;
use esp_idf_svc::hal::gpio::{Output, OutputPin};

use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{gpio::PinDriver, prelude::*},
    http::{
        server::{Configuration, EspHttpServer},
        Method,
    },
    io::Write as _,
};

use std::{thread::sleep, time::Duration};

#[derive(Debug)]
#[toml_cfg::toml_config]
pub struct Config {
    #[default("NOT SET")]
    wifi_ssid: &'static str,
    #[default("NOT SET")]
    wifi_psk: &'static str,
}

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    say_hello();

    let app_config = CONFIG;

    log::info!("WiFi creds: {}", app_config.wifi_ssid);

    let _sysloop = EspSystemEventLoop::take().expect("Cannot take SystemEventLoop");
    let peripherals = Peripherals::take().expect("Cannot take peripherals");

    // Connect to the Wi-Fi network
    // let _wifi = connect_to_wifi(
    //     app_config.wifi_ssid,
    //     app_config.wifi_psk,
    //     peripherals.modem,
    //     sysloop,
    // )
    // .expect("cannot connect to wifi");

    // // Set the HTTP server
    // let _http_server = setup_http_server();

    let mut sections = Sections::new(
        peripherals.pins.gpio14,
        peripherals.pins.gpio26,
        peripherals.pins.gpio27,
        peripherals.pins.gpio33,
    )
    .expect("Failed to setup GPIO");

    let rtc_clock = RTCClock::new(
        peripherals.pins.gpio21,
        peripherals.pins.gpio22,
        peripherals.pins.gpio23,
        peripherals.i2c0,
    )
    .expect("Unable to create RTC clock");

    rtc_clock.start();

    loop {
        sleep(Duration::from_millis(1000));
        log::info!("Toggle pins");
        sections.flowers.toggle().unwrap();
        sections.terrace.toggle().unwrap();
        sections.grass.toggle().unwrap();
        sections.vegs.toggle().unwrap();
    }
}

#[derive(Debug, PartialEq, Sequence)]
enum CurrentSection {
    Vegs,
    Flowers,
    Grass,
    Terrace,
    None,
}

struct Sections<
    VegsGPIO: OutputPin,
    TerraceGPIO: OutputPin,
    FlowersGPIO: OutputPin,
    GrassGPIO: OutputPin,
> {
    vegs: PinDriver<'static, VegsGPIO, Output>,
    terrace: PinDriver<'static, TerraceGPIO, Output>,
    flowers: PinDriver<'static, FlowersGPIO, Output>,
    grass: PinDriver<'static, GrassGPIO, Output>,

    current_section: CurrentSection,
}

impl<VegsGPIO: OutputPin, TerraceGPIO: OutputPin, FlowersGPIO: OutputPin, GrassGPIO: OutputPin>
    Sections<VegsGPIO, TerraceGPIO, FlowersGPIO, GrassGPIO>
{
    fn new(
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
            current_section: CurrentSection::None,
        };

        sections.vegs.set_low()?;
        sections.flowers.set_low()?;
        sections.terrace.set_low()?;
        sections.grass.set_low()?;

        Ok(sections)
    }
}

fn setup_http_server() -> EspHttpServer<'static> {
    let mut server =
        EspHttpServer::new(&Configuration::default()).expect("Cannot create the http server");
    // http://<sta ip>/ handler
    server
        .fn_handler("/", Method::Get, |request| -> anyhow::Result<()> {
            let html = "It works!";
            let mut response = request.into_ok_response()?;
            response.write_all(html.as_bytes())?;
            Ok(())
        })
        .expect("Cannot add the handler");

    server
}

fn say_hello() {
    log::info!(
        r#"
                .--.      .--.    ____     ,---------.      .-''-.   .-------.
                |  |_     |  |  .'  __ `.  \          \   .'_ _   \  |  _ _   \
                | _( )_   |  | /   '  \  \  `--.  ,---'  / ( ` )   ' | ( ' )  |
                |(_ o _)  |  | |___|  /  |     |   \    . (_ o _)  | |(_ o _) /
                | (_,_) \ |  |    _.-`   |     :_ _:    |  (_,_)___| | (_,_).' __
                |  |/    \|  | .'   _    |     (_I_)    '  \   .---. |  |\ \  |  |
                |  '  /\  `  | |  _( )_  |    (_(=)_)    \  `-'    / |  | \ `'   /
                |    /  \    | \ (_ o _) /     (_I_)      \       /  |  |  \    /
                `---'    `---`  '.(_,_).'      '---'       `'-..-'   ''-'   `'-'

                                ,---.    ,---.    ____     __
                                |    \  /    |    \   \   /  /
                                |  ,  \/  ,  |     \  _. /  '
                                |  |\_   /|  |      _( )_ .'
                                |  _( )_/ |  |  ___(_ o _)'
                                | (_ o _) |  | |   |(_,_)'
                                |  (_,_)  |  | |   `-'  /
                                |  |      |  |  \      /
                                '--'      '--'   `-..-'

          .-_'''-.       ____     .-------.      ______          .-''-.   ,---.   .--.
         '_( )_   \    .'  __ `.  |  _ _   \    |    _ `''.    .'_ _   \  |    \  |  |
        |(_ o _)|  '  /   '  \  \ | ( ' )  |    | _ | ) _  \  / ( ` )   ' |  ,  \ |  |
        . (_,_)/___|  |___|  /  | |(_ o _) /    |( ''_'  ) | . (_ o _)  | |  |\_ \|  |
        |  |  .-----.    _.-`   | | (_,_).' __  | . (_) `. | |  (_,_)___| |  _( )_\  |
        '  \  '-   .' .'   _    | |  |\ \  |  | |(_    ._) ' '  \   .---. | (_ o _)  |
         \  `-'`   |  |  _( )_  | |  | \ `'   / |  (_.\.' /   \  `-'    / |  (_,_)\  |
          \        /  \ (_ o _) / |  |  \    /  |       .'     \       /  |  |    |  |
           `'-...-'    '.(_,_).'  ''-'   `'-'   '-----'`        `'-..-'   '--'    '--'
"#
    );
}
