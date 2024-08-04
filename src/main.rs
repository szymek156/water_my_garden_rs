mod wifi;

use anyhow::Result;

use enum_iterator::Sequence;
use esp_idf_svc::hal::gpio::Output;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        gpio::{Gpio14, Gpio26, Gpio27, Gpio33, PinDriver, Pins},
        prelude::Peripherals,
    },
    http::{
        server::{Configuration, EspHttpServer},
        Method,
    },
    io::Write as _,
};

use std::{thread::sleep, time::Duration};
use wifi::connect_to_wifi;

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

    let sysloop = EspSystemEventLoop::take().expect("Cannot take SystemEventLoop");
    let peripherals = Peripherals::take().expect("Cannot take peripherals");

    // Connect to the Wi-Fi network
    let _wifi = connect_to_wifi(
        app_config.wifi_ssid,
        app_config.wifi_psk,
        peripherals.modem,
        sysloop,
    )
    .expect("cannot connect to wifi");

    // Set the HTTP server
    let _http_server = setup_http_server();

    let mut sections = Sections::new(peripherals.pins).expect("Failed to setup GPIO");

    // let i2c = peripherals.i2c0;
    // let sda = peripherals.pins.gpio21;
    // let scl = peripherals.pins.gpio22;
    // TODO: INT pin GPIO23

    // let config = I2cConfig::new().baudrate(400.kHz().into());
    // let mut i2c_dev = I2cDriver::new(i2c, sda, scl, &config).expect("Failed to create I2C driver");
    // let rtc = Ds323x::new_ds3231(i2c_dev);

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

struct Sections {
    vegs: PinDriver<'static, Gpio14, Output>,
    terrace: PinDriver<'static, Gpio26, Output>,
    flowers: PinDriver<'static, Gpio27, Output>,
    grass: PinDriver<'static, Gpio33, Output>,

    current_section: CurrentSection,
}

impl Sections {
    fn new(pins: Pins) -> Result<Self> {
        let mut sections = Self {
            vegs: PinDriver::output(pins.gpio14)?,
            terrace: PinDriver::output(pins.gpio26)?,
            flowers: PinDriver::output(pins.gpio27)?,
            grass: PinDriver::output(pins.gpio33)?,
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
