mod clock;
mod sections;
mod watering;
mod wifi;

use clock::ClockService;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::prelude::*,
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
    esp_idf_svc::log::set_target_level("water_my_garden_rs", log::LevelFilter::Debug).unwrap();

    // TODO: for now quick and dirty, figure something clever
    #[cfg(test)]
    execute_tests();
    #[cfg(not(test))]
    run();

    loop {
        sleep(Duration::from_millis(1000));
    }
}

#[cfg(test)]
fn execute_tests() {
    log::info!("Executing tests");
    watering::tests::example_valid_configuration_works();
    watering::tests::can_skip_a_section();
    watering::tests::can_skip_all_sections();
    log::info!("All tests passed!");
}

fn run() {
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
    let _watering_service_channel = watering_service.start();
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
