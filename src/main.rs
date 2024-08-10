mod clock;
mod sections;
mod watering;
mod wifi;

use anyhow::Result;

use clock::ClockService;

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
use sections::Sections;
use watering::WateringService;

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

    let _app_config = CONFIG;

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

    let sections_service = Sections::new(
        peripherals.pins.gpio14,
        peripherals.pins.gpio26,
        peripherals.pins.gpio27,
        peripherals.pins.gpio33,
    )
    .expect("Failed to setup Sections");

    let clock_service = ClockService::new(
        peripherals.pins.gpio21,
        peripherals.pins.gpio22,
        peripherals.pins.gpio23,
        peripherals.i2c0,
    )
    .expect("Failed to setup Clock");

    let clock_service_channel = clock_service.start();
    let sections_service_channel = sections_service.start();

    let watering_service = WateringService::new(clock_service_channel, sections_service_channel);
    let watering_service_channel = watering_service.start();

    loop {
        sleep(Duration::from_millis(1000));
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
