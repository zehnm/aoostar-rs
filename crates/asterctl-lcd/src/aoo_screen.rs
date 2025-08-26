// SPDX-License-Identifier: MIT OR Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025 Markus Zehnder

use crate::FakeSerialPort;
use crate::ToRgb565;

use anyhow::{Context, anyhow};
use bytes::{BufMut, BytesMut};
use log::{debug, error, info, warn};
use serialport::{SerialPort, SerialPortType};
use std::io::{Read, Write};
use std::thread::sleep;
use std::time::{Duration, Instant};

pub const DISPLAY_SIZE: (u32, u32) = (960, 376);

const SERIAL_RETRY: u8 = 3;
const UART_BAUDRATE: u32 = 1_500_000;

const USB_UART_VID: u16 = 0x416;
const USB_UART_PID: u16 = 0x90A1;

const IMG_CHUNK_SIZE: usize = 47;

static DISPLAY_OFF: [u8; 8] = [0xAA, 0x55, 0xAA, 0x55, 0x0A, 0x00, 0x00, 0x00];
static DISPLAY_ON: [u8; 8] = [0xAA, 0x55, 0xAA, 0x55, 0x0B, 0x00, 0x00, 0x00];

static HEADER_START: [u8; 16] = [
    0xAA, 0x55, 0xAA, 0x55, 0x05, 0x00, 0x00, 0x00, 0x04, 0x00, 0x0F, 0x2F, 0x00, 0x04, 0x0B, 0x00,
];
static HEADER_END: [u8; 8] = [0xAA, 0x55, 0xAA, 0x55, 0x06, 0x00, 0x00, 0x00];
static HEADER: [u8; 8] = [0xAA, 0x55, 0xAA, 0x55, 0x08, 0x00, 0x00, 0x00];

#[derive(Default)]
pub struct AooScreenBuilder {
    timeout: Option<Duration>,
    enable_cache: Option<bool>,
    no_init_check: Option<bool>,
}

#[allow(dead_code)]
impl AooScreenBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    /// Set the amount of time to wait to receive data before timing out. Defaults to 1 sec.
    pub fn timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = Some(timeout);
        self
    }

    /// Cache previous frame sent to display for future diff updates. Enabled by default.
    pub fn enable_cache(&mut self, enable: bool) -> &mut Self {
        self.enable_cache = Some(enable);
        self
    }

    /// Disable LCD initialization check and only write data to the display. Defaults to false.
    pub fn no_init_check(&mut self, no_check: bool) -> &mut Self {
        self.no_init_check = Some(no_check);
        self
    }

    /// Open the default AOOSTAR LCD USB UART device 416:90A1.
    pub fn open_default(self) -> anyhow::Result<AooScreen> {
        self.open_usb(USB_UART_VID, USB_UART_PID)
    }

    /// Simulate the LCD device. No real device or serial port is required.
    pub fn simulate(self) -> anyhow::Result<AooScreen> {
        Ok(AooScreen {
            port: Some(Box::new(FakeSerialPort::new())),
            enable_cache: self.enable_cache.unwrap_or(true),
            prev_frame: None,
            no_init_check: self.no_init_check.unwrap_or(false),
        })
    }

    /// Open the specified USB UART device id. Format: vid:pid
    pub fn open_usb_id(self, id: &str) -> anyhow::Result<AooScreen> {
        let (vid, pid) = id
            .split_once(':')
            .with_context(|| "Error parsing serial port ID. Expected `vid:pid` format.")?;
        self.open_usb(u16::from_str_radix(vid, 16)?, u16::from_str_radix(pid, 16)?)
    }

    /// Open the specified USB UART
    pub fn open_usb(self, vid: u16, pid: u16) -> anyhow::Result<AooScreen> {
        let serial_dev = find_usb_serial_port(vid, pid)?;
        self.open_device(&serial_dev)
    }

    /// Open the specified serial device
    pub fn open_device(self, device: &str) -> anyhow::Result<AooScreen> {
        let port = serialport::new(device, UART_BAUDRATE)
            .timeout(self.timeout.unwrap_or(Duration::from_millis(1000)))
            .open()
            .with_context(|| format!("Error opening serial port: {device}"))?;

        info!(
            "Opened serial port {device}: baud={}, {}:{}:{}",
            port.baud_rate()?,
            port.data_bits()?,
            port.parity()?,
            port.stop_bits()?
        );

        Ok(AooScreen {
            port: Some(port),
            enable_cache: self.enable_cache.unwrap_or(true),
            prev_frame: None,
            no_init_check: self.no_init_check.unwrap_or(false),
        })
    }
}

pub struct AooScreen {
    port: Option<Box<dyn SerialPort>>,
    enable_cache: bool,
    prev_frame: Option<BytesMut>,
    no_init_check: bool,
}

#[allow(dead_code)]
impl AooScreen {
    pub fn init(&mut self) -> anyhow::Result<()> {
        let port = self.port.as_mut().ok_or(anyhow!("LCD port not open"))?;

        port.write(&DISPLAY_ON)
            .with_context(|| "Error sending display on command")?;

        if self.no_init_check {
            warn!("Test mode: only writing to the display");
        } else {
            // quick and dirty response check as in the original app
            sleep(Duration::from_secs(1));

            let available = port
                .bytes_to_read()
                .with_context(|| "Failed to get available bytes from serial port")?;
            if available == 0 {
                return Err(anyhow!("Initialization failed, no response received"));
            }
            let mut serial_buf: Vec<u8> = vec![0; available as usize];
            port.read(serial_buf.as_mut_slice())
                .with_context(|| "Failed to read from serial port")?;

            let marker = b'A';
            if !serial_buf.contains(&marker) {
                return Err(anyhow!(
                    "Initialization failed, received: {}",
                    String::from_utf8_lossy(&serial_buf)
                ));
            }
        }

        info!("Display initialized!");

        Ok(())
    }

    pub fn close(&mut self) {
        if self.port.is_some() {
            if let Err(e) = self.off() {
                warn!("Failed to close display: {e}");
            }
            self.port = None;
        }
    }

    pub fn on(&mut self) -> anyhow::Result<()> {
        self.send(&DISPLAY_ON)
            .with_context(|| "Failed to send display on")
    }

    pub fn off(&mut self) -> anyhow::Result<()> {
        self.send(&DISPLAY_OFF)
            .with_context(|| "Failed to send display off")
    }

    pub fn send_image(&mut self, image: impl ToRgb565) -> anyhow::Result<()> {
        let img_rgb565 = image.to_rgb565_le();
        debug!(
            "Start sending image (size {}) {} cache... ",
            img_rgb565.len(),
            if self.enable_cache && self.prev_frame.is_some() {
                "with"
            } else {
                "without"
            }
        );

        let start_time = Instant::now();
        self.send(&HEADER_START)
            .with_context(|| "Failed to send header start")?;

        let mut buf = BytesMut::with_capacity(HEADER.len() + 4 + IMG_CHUNK_SIZE);
        let mut sent_chunks = 0;
        for (idx, chunk) in img_rgb565.chunks(IMG_CHUNK_SIZE).enumerate() {
            let offset = idx * IMG_CHUNK_SIZE;

            if self.enable_cache
                && let Some(cache) = self.prev_frame.as_mut()
            {
                let offset = idx * IMG_CHUNK_SIZE;
                if offset + IMG_CHUNK_SIZE <= cache.len()
                    && cache[offset..offset + IMG_CHUNK_SIZE].eq(chunk)
                {
                    // Block is unchanged from the previous frame; skip sending
                    continue;
                }
            }

            buf.clear();
            buf.extend(&HEADER);
            buf.put_u32_le(offset as u32);
            buf.extend(chunk);

            self.send(&buf)
                .with_context(|| format!("Failed to send image data chunk {idx}"))?;
            sent_chunks += 1;
        }

        self.send(&HEADER_END)
            .with_context(|| "Failed to send header end")?;

        if self.enable_cache {
            self.prev_frame.replace(img_rgb565);
        }

        debug!(
            "Image sent: {}ms, {sent_chunks} chunks",
            start_time.elapsed().as_millis()
        );

        Ok(())
    }

    pub fn enable_cache(&mut self, enable: bool) {
        self.enable_cache = enable;
        if !enable {
            self.clear_cache();
        }
    }

    pub fn is_cache_enabled(&self) -> bool {
        self.enable_cache
    }

    pub fn clear_cache(&mut self) {
        self.prev_frame = None;
    }

    fn send(&mut self, data: &[u8]) -> anyhow::Result<()> {
        // TODO not sure if retry logic is required. Need a real device to test...
        let mut retry = 0;

        let port = self.port.as_mut().ok_or(anyhow!("LCD port not open"))?;

        loop {
            return match port.write_all(data) {
                Ok(()) => {
                    port.flush()?;
                    Ok(())
                }
                Err(e) => {
                    debug!(
                        "Bytes queued to send: {}",
                        port.bytes_to_write()
                            .with_context(|| "Error calling bytes_to_write")?
                    );
                    if retry < SERIAL_RETRY {
                        warn!("Failed to write to display, retrying! Error: {e}");
                        retry += 1;
                        continue;
                    }
                    error!("Failed to write to display: {e}");
                    Err(e.into())
                }
            };
        }
    }
}

pub fn find_usb_serial_port(vid: u16, pid: u16) -> serialport::Result<String> {
    info!("Looking for USB serial port {vid:x}:{pid:x}");
    let ports = serialport::available_ports()?;
    for p in ports {
        debug!("Found serial port: {}", p.port_name);
        if let SerialPortType::UsbPort(info) = p.port_type
            && info.pid == pid
            && info.vid == vid
        {
            return Ok(p.port_name);
        }
    }

    Err(serialport::Error::new(
        serialport::ErrorKind::NoDevice,
        format!("USB serial port {vid:x}:{pid:x} not found"),
    ))
}
