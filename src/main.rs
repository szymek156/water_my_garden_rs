mod wifi;

use anyhow::anyhow;
use anyhow::Result;
use chrono::TimeDelta;
use ds323x::ic::DS3231;
use ds323x::interface::I2cInterface;
use ds323x::DateTimeAccess;
use ds323x::Ds323x;

use ds323x::NaiveDateTime;
use enum_iterator::Sequence;
use esp_idf_svc::hal::gpio::{InputPin, Output, OutputPin};

use esp_idf_svc::hal::i2c::{I2c, I2cConfig, I2cDriver};
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        gpio::PinDriver,
        prelude::*,
        task::notification::{Notification, Notifier},
    },
    http::{
        server::{Configuration, EspHttpServer},
        Method,
    },
    io::Write as _,
};

use std::num::NonZeroU32;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
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

    let mut rtc_clock = RTCClock::new(
        peripherals.pins.gpio21,
        peripherals.pins.gpio22,
        peripherals.i2c0,
    )
    .expect("Unable to create RTC clock");

    // This is a thin wrapper over the FreeRTOS task
    std::thread::spawn(move || {
        log::info!("Hello From int handling thread!");

        // int_pin:23
        let mut int_pin = PinDriver::input(peripherals.pins.gpio23).unwrap();

        log::info!("int level: {:?}", int_pin.get_level());

        // INT pin on RTC is high by default, listen on falling edge
        int_pin
            .set_interrupt_type(esp_idf_svc::hal::gpio::InterruptType::NegEdge)
            .unwrap();
        int_pin
            .set_pull(esp_idf_svc::hal::gpio::Pull::Down)
            .unwrap();

        let notification = Notification::new();
        let notifier = notification.notifier();

        unsafe {
            int_pin
                .subscribe(move || {
                    static mut INT_COUNT: u32 = 1;

                    notifier.notify_and_yield(NonZeroU32::new(INT_COUNT).unwrap());

                    INT_COUNT += 1;
                })
                .unwrap()
        };

        int_pin.enable_interrupt().unwrap();

        while let Some(msg) = notification.wait(esp_idf_svc::hal::delay::BLOCK) {
            log::info!("Got interrupt notification! #{msg}");

            int_pin.enable_interrupt().unwrap();
        }
    });
    // unsafe { int_pin.subscribe(handle_interrupt).unwrap() };

    rtc_clock.test_arming_alarm();

    loop {
        sleep(Duration::from_millis(1000));
        log::info!("Toggle pins");
        sections.flowers.toggle().unwrap();
        sections.terrace.toggle().unwrap();
        sections.grass.toggle().unwrap();
        sections.vegs.toggle().unwrap();

        log::info!(
            "alarm1 matched {} ",
            rtc_clock.rtc.has_alarm1_matched().unwrap()
        );
        log::info!(
            "alarm2 matched {} ",
            rtc_clock.rtc.has_alarm2_matched().unwrap()
        );

        if rtc_clock.rtc.has_alarm1_matched().unwrap() {
            rtc_clock.test_arming_alarm();
            rtc_clock.rtc.clear_alarm1_matched_flag().unwrap();
        }



        log::info!("{}", rtc_clock.get_current_datetime().unwrap());
        // log::info!("int level: {:?}", int_pin.get_level());
    }
}

struct RTCClock {
    rtc: Ds323x<I2cInterface<I2cDriver<'static>>, DS3231>,
}

impl RTCClock {
    fn new(
        sda_pin: impl Peripheral<P = impl OutputPin + InputPin> + 'static,
        scl_pin: impl Peripheral<P = impl OutputPin + InputPin> + 'static,
        i2c: impl Peripheral<P = impl I2c> + 'static,
    ) -> Result<Self> {
        // TODO: INT pin GPIO23
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
        Ok(Self { rtc })
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
