use std::time::Duration;

use chrono::NaiveTime;
use embedded_svc::{
    http::{Headers, Method},
    io::{Read, Write},
};
use esp_idf_svc::http::server::EspHttpConnection;
use esp_idf_svc::http::server::Request;
use esp_idf_svc::{
    http::server::{Configuration, EspHttpServer},
    io::Write as _,
};
use log::info;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;

use crate::{
    clock::{ClockServiceChannel, ClockStatus},
    sections::{Section, SectionDuration},
    watering::{WateringServiceChannel, WateringServiceMessage, WateringStatus},
};
use anyhow::{anyhow, Context};

#[derive(Debug, Serialize)]
pub struct SystemStatus {
    watering: WateringStatus,
    clock: ClockStatus,
}

pub fn setup_http_server(
    clock_service_channel: ClockServiceChannel,
    watering_service_channel: WateringServiceChannel,
) -> anyhow::Result<EspHttpServer<'static>> {
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
        .context("handler /")?;

    {
        let watering_tx = watering_service_channel.clone();
        let clock_tx = clock_service_channel.clone();
        server
            .fn_handler("/status", Method::Get, move |req| {
                status(req, &watering_tx, &clock_tx)
            })
            .context("handler /status")?;
    }

    {
        let watering_tx = watering_service_channel.clone();
        server
            .fn_handler("/start_watering_at", Method::Post, move |req| {
                handle_start_watering_at(req, &watering_tx)
            })
            .context("handler /start_watering_at")?;
    }

    {
        let watering_tx = watering_service_channel.clone();
        server
            .fn_handler("/disable_watering", Method::Post, move |req| {
                disable_watering(req, &watering_tx)
            })
            .context("handler /disable_watering")?;
    }

    {
        let watering_tx = watering_service_channel.clone();
        server
            .fn_handler("/set_section_duration", Method::Post, move |req| {
                set_section_duration(req, &watering_tx)
            })
            .context("handler /set_section_duration")?;
    }

    {
        let watering_tx = watering_service_channel.clone();
        server
            .fn_handler("/close_all_valves", Method::Post, move |req| {
                close_all_valves(req, &watering_tx)
            })
            .context("handler /close_all_valves")?;
    }

    {
        let watering_tx = watering_service_channel.clone();
        server
            .fn_handler("/enable_section_for", Method::Post, move |req| {
                enable_section_for(req, &watering_tx)
            })
            .context("handler /enable_section_for")?;
    }

    Ok(server)
}

// Max payload length
const MAX_LEN: usize = 128;

fn status(
    req: Request<&mut EspHttpConnection<'_>>,
    watering_tx: &WateringServiceChannel,
    clock_tx: &ClockServiceChannel,
) -> anyhow::Result<()> {
    match get_system_status(watering_tx, clock_tx) {
        Ok(status) => {
            let system_json = serde_json::to_string_pretty(&status)?;

            req.into_ok_response()?.write_all(system_json.as_bytes())?;
        }
        Err(err) => req
            .into_status_response(500)?
            .write_all(err.to_string().as_bytes())?,
    };

    Ok(())
}

fn get_system_status(
    watering_tx: &WateringServiceChannel,
    clock_tx: &ClockServiceChannel,
) -> anyhow::Result<SystemStatus> {
    let (tx, rx) = std::sync::mpsc::channel();
    watering_tx
        .send(WateringServiceMessage::GetStatus(tx))
        .context("while sending get status to watering service")?;

    let watering_status = rx
        .recv_timeout(Duration::from_secs(10))
        .context("while receiving status from watering service")?;

    let (tx, rx) = std::sync::mpsc::channel();
    clock_tx
        .send(crate::clock::ClockServiceMessage::GetStatus(tx))
        .context("while sending get status to clock service")?;
    let clock_status = rx
        .recv_timeout(Duration::from_secs(10))
        .context("while receiving status from clock service")?;

    Ok(SystemStatus {
        watering: watering_status,
        clock: clock_status,
    })
}

#[derive(Deserialize)]
struct StartWateringAtReq {
    time: NaiveTime,
}

fn handle_start_watering_at(
    mut req: Request<&mut EspHttpConnection<'_>>,
    watering_tx: &WateringServiceChannel,
) -> anyhow::Result<()> {
    match get_body::<StartWateringAtReq>(&mut req) {
        Ok(body) => {
            watering_tx.send(WateringServiceMessage::StartWateringAt(body.time))?;
            req.into_ok_response()?.write_all("OK!".as_bytes())?;
        }
        Err(err) => {
            req.into_status_response(400)?
                .write_all(err.to_string().as_bytes())?;
        }
    };

    Ok(())
}

#[derive(Deserialize)]
struct SetSectionDurationReq {
    section: Section,
    duration: SectionDuration,
}

fn set_section_duration(
    mut req: Request<&mut EspHttpConnection<'_>>,
    watering_tx: &WateringServiceChannel,
) -> anyhow::Result<()> {
    match get_body::<SetSectionDurationReq>(&mut req) {
        Ok(body) => {
            watering_tx.send(WateringServiceMessage::SetSectionDuration(
                body.section,
                body.duration,
            ))?;

            req.into_ok_response()?.write_all("OK!".as_bytes())?;
        }
        Err(err) => {
            req.into_status_response(400)?
                .write_all(err.to_string().as_bytes())?;
        }
    };

    Ok(())
}

fn disable_watering(
    req: Request<&mut EspHttpConnection<'_>>,
    watering_tx: &WateringServiceChannel,
) -> anyhow::Result<()> {
    watering_tx.send(WateringServiceMessage::DisableWatering)?;
    req.into_ok_response()?.write_all("OK!".as_bytes())?;

    Ok(())
}

fn close_all_valves(
    req: Request<&mut EspHttpConnection<'_>>,
    watering_tx: &WateringServiceChannel,
) -> anyhow::Result<()> {
    watering_tx.send(WateringServiceMessage::CloseAllValves)?;
    req.into_ok_response()?.write_all("OK!".as_bytes())?;

    Ok(())
}

#[derive(Deserialize)]
struct EnableSectionForReq {
    section: Section,
    duration: SectionDuration,
}

fn enable_section_for(
    mut req: Request<&mut EspHttpConnection<'_>>,
    watering_tx: &WateringServiceChannel,
) -> anyhow::Result<()> {
    match get_body::<EnableSectionForReq>(&mut req) {
        Ok(body) => {
            watering_tx.send(WateringServiceMessage::EnableSectionFor(
                body.section,
                body.duration,
            ))?;
            req.into_ok_response()?.write_all("OK!".as_bytes())?;
        }
        Err(err) => {
            req.into_status_response(400)?
                .write_all(err.to_string().as_bytes())?;
        }
    };

    Ok(())
}

fn get_body<T: DeserializeOwned>(
    req: &mut Request<&mut EspHttpConnection>,
) -> Result<T, anyhow::Error> {
    let len = req.content_len().unwrap_or(0) as usize;
    info!("Content len {len}");
    if len > MAX_LEN {
        return Err(anyhow!("Request too big"));
    }
    let mut buf = vec![0; len];
    req.read_exact(&mut buf)?;
    let body = serde_json::from_slice(&buf)?;
    Ok(body)
}
