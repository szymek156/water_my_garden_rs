use chrono::NaiveTime;
use embedded_svc::{
    http::{Headers, Method},
    io::{Read, Write},
};
use esp_idf_svc::http::server::EspHttpConnection;
use esp_idf_svc::http::server::Request;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::prelude::*,
    http::server::{Configuration, EspHttpServer},
    io::Write as _,
};
use log::info;
use serde::{de::DeserializeOwned, Deserialize};

use crate::{
    clock::ClockServiceChannel,
    sections::{Section, SectionDuration},
    watering::WateringServiceChannel,
};
use anyhow::{anyhow, Context};

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
        server
            .fn_handler("/start_watering_at", Method::Post, move |req| {
                handle_start_watering_at(req, &watering_tx)
            })
            .context("handler /start_watering_at")?;
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
            watering_tx.send(crate::watering::WateringServiceMessage::StartWateringAt(
                body.time,
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
            watering_tx.send(crate::watering::WateringServiceMessage::SetSectionDuration(
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

fn close_all_valves(
    req: Request<&mut EspHttpConnection<'_>>,
    watering_tx: &WateringServiceChannel,
) -> anyhow::Result<()> {
    watering_tx.send(crate::watering::WateringServiceMessage::CloseAllValves)?;
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
            watering_tx.send(crate::watering::WateringServiceMessage::EnableSectionFor(
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
